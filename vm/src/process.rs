//! Lightweight, isolated processes for running code concurrently.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::chunk::Chunk;
use crate::config::Config;
use crate::execution_context::ExecutionContext;
use crate::generator::{Generator, RcGenerator};
use crate::immix::block_list::BlockList;
use crate::immix::copy_object::CopyObject;
use crate::immix::global_allocator::RcGlobalAllocator;
use crate::immix::local_allocator::LocalAllocator;
use crate::object_pointer::{ObjectPointer, ObjectPointerPointer};
use crate::object_value;
use crate::runtime_error::RuntimeError;
use crate::scheduler::timeouts::Timeout;
use crate::vm::state::State;
use num_bigint::BigInt;
use num_traits::FromPrimitive;
use parking_lot::{Mutex, MutexGuard};
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::i64;
use std::mem;
use std::ops::Drop;

/// A one-time single-producer single-consumer queue used for storing the result
/// of an process message.
pub struct Future {
    /// The process that owns the future.
    ///
    /// When writing the result to a future, this process may need to be woken
    /// up if it's waiting for the result.
    reply_to: RcProcess,

    /// A boolean indicating if the consuming side of the future has been
    /// dropped.
    disconnected: bool,

    /// A boolean indicating that the process is waiting for this future to be
    /// completed.
    ///
    /// When set to `true`, the `reply_to` process must be rescheduled.
    waiting: bool,

    /// A boolean indicating that the result was produced by throwing an error,
    /// instead of returning it.
    thrown: bool,

    /// The result of a message, owned by the process to reply to.
    ///
    /// This pointer is NULL when the result isn't available yet.
    result: ObjectPointer,
}

pub type RcFuture = ArcWithoutWeak<Mutex<Future>>;

impl Future {
    pub fn new(reply_to: RcProcess) -> (FutureProducer, FutureConsumer) {
        let future = ArcWithoutWeak::new(Mutex::new(Self {
            reply_to,
            disconnected: false,
            waiting: false,
            thrown: false,
            result: ObjectPointer::null(),
        }));

        (
            FutureProducer {
                future: future.clone(),
            },
            FutureConsumer { future },
        )
    }

    pub fn no_longer_waiting(&mut self) {
        self.waiting = false;
    }

    pub fn waiting(&mut self) {
        if self.result.is_null() {
            self.waiting = true;
        }
    }

    pub fn result(&self) -> Option<ObjectPointer> {
        if self.result.is_null() {
            None
        } else {
            Some(self.result)
        }
    }

    pub fn result_pointer_pointer(&self) -> Option<ObjectPointerPointer> {
        if self.result.is_null() {
            None
        } else {
            Some(self.result.pointer())
        }
    }
}

/// The consuming side of a future.
///
/// The consumer is owned/used by the process that originally sent the message,
/// and eventually wants the result.
pub struct FutureConsumer {
    future: RcFuture,
}

impl FutureConsumer {
    pub fn lock(&self) -> MutexGuard<Future> {
        self.future.lock()
    }
}

impl Drop for FutureConsumer {
    fn drop(&mut self) {
        self.lock().disconnected = true;
    }
}

/// The producing side of a future.
///
/// The producer is owned/used by the process that produces a result in response a
/// message.
pub struct FutureProducer {
    future: RcFuture,
}

impl FutureProducer {
    pub fn set_result(
        &self,
        producer: &RcProcess,
        result: ObjectPointer,
        thrown: bool,
    ) -> Result<Option<(RescheduleRights, RcProcess)>, String> {
        let mut future = self.future.lock();

        // If the consumer has been dropped, there's no point in sending the
        // result back.
        if future.disconnected {
            return Ok(None);
        }

        future.thrown = thrown;

        // When the writing process owns the future, there's no need for copying
        // or rescheduling.
        if producer == &future.reply_to {
            future.result = result;

            return Ok(None);
        }

        // We lock the shared data first so our objects don't get garbage
        // collected while we're copying them to the target process.
        let reply_to = future.reply_to.clone();
        let resched = {
            let mut shared = reply_to.shared_data.lock();

            future.result = future
                .reply_to
                .local_data_mut()
                .allocator
                .copy_object(result)?;

            shared.try_reschedule_for_future()
        };

        Ok(Some((resched, reply_to)))
    }
}

/// A message and its arguments to send to an process.
pub struct Message {
    /// The method to run.
    method: ObjectPointer,

    /// The future to write the result back to.
    future: FutureProducer,

    /// The arguments to pass to the method. These have already been copied onto
    /// the process's heap.
    arguments: Chunk<ObjectPointer>,
}

impl Message {
    pub fn new(
        method: ObjectPointer,
        arguments: Chunk<ObjectPointer>,
        future: FutureProducer,
    ) -> Self {
        Message {
            method,
            arguments,
            future,
        }
    }
}

/// A message that is being acted upon/executed.
pub struct Request {
    /// The future to write the result to.
    future: FutureProducer,

    /// The generator currently running for this request.
    generator: RcGenerator,
}

impl Request {
    pub fn from_message(
        message: Message,
        receiver: ObjectPointer,
    ) -> Result<Request, String> {
        let block = message.method.block_value()?;
        let mut context =
            ExecutionContext::from_block_with_receiver(&block, receiver);

        for index in 0..message.arguments.len() {
            context.set_local(index as u16, message.arguments[index]);
        }

        let generator = Generator::running(Box::new(context));
        let future = message.future;

        Ok(Self { future, generator })
    }
}

pub struct Mailbox {
    /// The messages stored in this mailbox.
    messages: VecDeque<Message>,
}

impl Mailbox {
    pub fn new() -> Self {
        Mailbox {
            messages: VecDeque::new(),
        }
    }

    pub fn send(&mut self, message: Message) {
        self.messages.push_back(message);
    }

    pub fn receive(&mut self) -> Option<Message> {
        self.messages.pop_front()
    }

    pub fn each_pointer<F>(&self, mut callback: F)
    where
        F: FnMut(ObjectPointerPointer),
    {
        for message in &self.messages {
            for index in 0..message.arguments.len() {
                callback(message.arguments[index].pointer());
            }
        }
    }
}

/// The status of a process, represented as a set of bits.
// TODO: use the bitflags crate
pub struct ProcessStatus {
    /// The bits used to indicate the status of the process.
    ///
    /// Multiple bits may be set in order to combine different statuses. For
    /// example, if the main process is blocking it will set both bits.
    bits: u8,
}

impl ProcessStatus {
    /// A regular process.
    const NORMAL: u8 = 0b0;

    /// The main process.
    const MAIN: u8 = 0b1;

    /// The process is performing a blocking operation.
    const BLOCKING: u8 = 0b10;

    /// The process is terminated.
    const TERMINATED: u8 = 0b100;

    /// The process is waiting for a message.
    const WAITING_FOR_MESSAGE: u8 = 0b1000;

    /// The process is waiting for a future.
    const WAITING_FOR_FUTURE: u8 = 0b10000;

    /// The process is simply sleeping for a certain amount of time.
    const SLEEPING: u8 = 0b100000;

    /// The process was rescheduled after a timeout expired.
    const TIMEOUT_EXPIRED: u8 = 0b1000000;

    /// The process is waiting for something, or suspended for a period of time
    const WAITING: u8 =
        Self::WAITING_FOR_FUTURE | Self::WAITING_FOR_MESSAGE | Self::SLEEPING;

    pub fn new() -> Self {
        Self { bits: Self::NORMAL }
    }

    fn set_main(&mut self) {
        self.update_bits(Self::MAIN, true);
    }

    fn is_main(&self) -> bool {
        self.bit_is_set(Self::MAIN)
    }

    fn set_blocking(&mut self, enable: bool) {
        self.update_bits(Self::BLOCKING, enable);
    }

    fn is_blocking(&self) -> bool {
        self.bit_is_set(Self::BLOCKING)
    }

    fn set_terminated(&mut self) {
        self.update_bits(Self::TERMINATED, true);
    }

    fn is_terminated(&self) -> bool {
        self.bit_is_set(Self::TERMINATED)
    }

    fn set_waiting_for_message(&mut self, enable: bool) {
        self.update_bits(Self::WAITING_FOR_MESSAGE, enable);
    }

    fn is_waiting_for_message(&self) -> bool {
        self.bit_is_set(Self::WAITING_FOR_MESSAGE)
    }

    fn set_waiting_for_future(&mut self, enable: bool) {
        self.update_bits(Self::WAITING_FOR_FUTURE, enable);
    }

    fn is_waiting_for_future(&self) -> bool {
        self.bit_is_set(Self::WAITING_FOR_FUTURE)
    }

    fn is_waiting(&self) -> bool {
        (self.bits & Self::WAITING) != 0
    }

    fn no_longer_waiting(&mut self) {
        self.update_bits(Self::WAITING, false);
    }

    fn set_timeout_expired(&mut self, enable: bool) {
        self.update_bits(Self::TIMEOUT_EXPIRED, enable)
    }

    fn set_sleeping(&mut self, enable: bool) {
        self.update_bits(Self::SLEEPING, enable);
    }

    fn is_sleeping(&self) -> bool {
        self.bit_is_set(Self::SLEEPING)
    }

    fn timeout_expired(&self) -> bool {
        self.bit_is_set(Self::TIMEOUT_EXPIRED)
    }

    fn update_bits(&mut self, mask: u8, enable: bool) {
        self.bits = if enable {
            self.bits | mask
        } else {
            self.bits & !mask
        };
    }

    fn bit_is_set(&self, bit: u8) -> bool {
        self.bits & bit == bit
    }
}

/// An enum describing what rights a thread was given when trying to reschedule
/// a process.
#[derive(PartialEq, Eq)]
pub enum RescheduleRights {
    /// The rescheduling rights were not obtained.
    Failed,

    /// The rescheduling rights were obtained.
    Acquired,

    /// The rescheduling rights were obtained, and the process was using a
    /// timeout.
    AcquiredWithTimeout,
}

impl RescheduleRights {
    pub fn are_acquired(&self) -> bool {
        match self {
            RescheduleRights::Failed => false,
            _ => true,
        }
    }
}

pub struct LocalData {
    /// The process-local memory allocator.
    pub allocator: LocalAllocator,

    /// The heap allocated object wrapping this process.
    ///
    /// This object is used as `self` for the process itself. Since this object
    /// depends on the process itself, it must be set manually after allocating
    /// an process.
    self_object: ObjectPointer,

    /// The request that's currently being executed.
    request: Option<Request>,

    /// The ID of the thread this process is pinned to.
    thread_id: Option<u8>,
}

pub struct SharedData {
    /// The mailbox of this process.
    mailbox: Mailbox,

    /// The status of the process.
    status: ProcessStatus,

    /// The timeout this process is suspended with, if any.
    ///
    /// If missing and the process is suspended, it means the process is suspended
    /// indefinitely.
    timeout: Option<ArcWithoutWeak<Timeout>>,
}

impl SharedData {
    pub fn mailbox_mut(&mut self) -> &mut Mailbox {
        &mut self.mailbox
    }

    pub fn has_same_timeout(&self, timeout: &ArcWithoutWeak<Timeout>) -> bool {
        self.timeout
            .as_ref()
            .map(|t| t.as_ptr() == timeout.as_ptr())
            .unwrap_or(false)
    }

    pub fn timeout_expired(&mut self) -> bool {
        if self.status.timeout_expired() {
            self.status.set_timeout_expired(false);
            true
        } else {
            false
        }
    }

    pub fn park_for_future(
        &mut self,
        timeout: Option<ArcWithoutWeak<Timeout>>,
    ) {
        self.timeout = timeout;
        self.status.set_waiting_for_future(true);
    }

    pub fn park_for_message(&mut self) {
        self.status.set_waiting_for_message(true);
    }

    pub fn park(&mut self, timeout: ArcWithoutWeak<Timeout>) {
        self.timeout = Some(timeout);
        self.status.set_sleeping(true);
    }

    pub fn try_reschedule_after_timeout(&mut self) -> RescheduleRights {
        if !self.status.is_waiting() {
            return RescheduleRights::Failed;
        }

        if !self.status.is_sleeping() {
            // If the process was waiting for something (e.g. a future), we need
            // to mark it as expired. If the process simply paused itself for a
            // period of time, this is unnecessary.
            self.status.set_timeout_expired(true);
        }

        self.status.no_longer_waiting();

        if self.timeout.take().is_some() {
            RescheduleRights::AcquiredWithTimeout
        } else {
            RescheduleRights::Acquired
        }
    }

    fn try_reschedule_for_message(&mut self) -> RescheduleRights {
        if !self.status.is_waiting_for_message() {
            return RescheduleRights::Failed;
        }

        self.status.set_waiting_for_message(false);
        RescheduleRights::Acquired
    }

    fn try_reschedule_for_future(&mut self) -> RescheduleRights {
        if !self.status.is_waiting_for_future() {
            return RescheduleRights::Failed;
        }

        self.status.set_waiting_for_future(false);

        if self.timeout.take().is_some() {
            RescheduleRights::AcquiredWithTimeout
        } else {
            RescheduleRights::Acquired
        }
    }
}

pub type RcProcess = ArcWithoutWeak<Process>;

pub struct Process {
    /// Data stored in a process that should only be modified by a single thread
    /// at once.
    local_data: UnsafeCell<LocalData>,

    /// Data that can be accessed by multiple threads, but only by one thread at
    /// a time.
    shared_data: Mutex<SharedData>,
}

impl Process {
    pub fn new(
        global_allocator: RcGlobalAllocator,
        config: &Config,
    ) -> RcProcess {
        let local_data = LocalData {
            allocator: LocalAllocator::new(global_allocator, config),
            request: None,
            self_object: ObjectPointer::null(),
            thread_id: None,
        };

        let shared_data = SharedData {
            mailbox: Mailbox::new(),
            status: ProcessStatus::new(),
            timeout: None,
        };

        ArcWithoutWeak::new(Process {
            local_data: UnsafeCell::new(local_data),
            shared_data: Mutex::new(shared_data),
        })
    }

    pub fn set_main(&self) {
        self.shared_data.lock().status.set_main();
    }

    pub fn is_main(&self) -> bool {
        self.shared_data.lock().status.is_main()
    }

    pub fn set_blocking(&self, enable: bool) {
        self.shared_data.lock().status.set_blocking(enable);
    }

    pub fn is_blocking(&self) -> bool {
        self.shared_data.lock().status.is_blocking()
    }

    pub fn is_terminated(&self) -> bool {
        self.shared_data.lock().status.is_terminated()
    }

    pub fn thread_id(&self) -> Option<u8> {
        self.local_data().thread_id
    }

    pub fn set_thread_id(&self, id: u8) {
        self.local_data_mut().thread_id = Some(id);
    }

    pub fn unset_thread_id(&self) {
        self.local_data_mut().thread_id = None;
    }

    pub fn is_pinned(&self) -> bool {
        self.thread_id().is_some()
    }

    #[cfg_attr(feature = "cargo-clippy", allow(mut_from_ref))]
    pub fn local_data_mut(&self) -> &mut LocalData {
        unsafe { &mut *self.local_data.get() }
    }

    pub fn local_data(&self) -> &LocalData {
        unsafe { &*self.local_data.get() }
    }

    pub fn shared_data(&self) -> MutexGuard<SharedData> {
        self.shared_data.lock()
    }

    pub fn push_context(&self, context: ExecutionContext) {
        self.local_data_mut()
            .generator
            .as_mut()
            .expect("Can't push an execution context without a generator")
            .push_context(context);
    }

    pub fn resume_generator(&self, mut generator: RcGenerator) {
        let local_data = self.local_data_mut();

        if let Some(target) = local_data.generator.as_mut() {
            mem::swap(target, &mut generator);
            target.set_parent(generator);
        } else {
            local_data.generator = Some(generator);
        }
    }

    /// Pops an execution context.
    ///
    /// This method returns true if we're at the top of the execution context
    /// stack.
    pub fn pop_context(&self) -> bool {
        let local_data = self.local_data_mut();

        let gen = if let Some(gen) = local_data.generator.as_mut() {
            gen
        } else {
            return true;
        };

        if gen.pop_context() {
            gen.set_finished();

            if let Some(parent) = gen.take_parent() {
                local_data.generator = Some(parent);
                false
            } else {
                local_data.generator = None;
                true
            }
        } else {
            false
        }
    }

    /// Yields a value to the generator.
    ///
    /// If a value is yielded, `true` is returned.
    pub fn yield_value(&self, value: ObjectPointer) -> bool {
        let local_data = self.local_data_mut();
        let gen = if let Some(gen) = local_data.generator.as_mut() {
            gen
        } else {
            return false;
        };

        gen.yield_value(value);

        if let Some(parent) = gen.take_parent() {
            local_data.generator = Some(parent);

            true
        } else {
            false
        }
    }

    pub fn allocate_empty(&self) -> ObjectPointer {
        self.local_data_mut().allocator.allocate_empty()
    }

    pub fn allocate_usize(
        &self,
        value: usize,
        prototype: ObjectPointer,
    ) -> ObjectPointer {
        self.allocate_u64(value as u64, prototype)
    }

    pub fn allocate_i64(
        &self,
        value: i64,
        prototype: ObjectPointer,
    ) -> ObjectPointer {
        if ObjectPointer::integer_too_large(value) {
            self.allocate(object_value::integer(value), prototype)
        } else {
            ObjectPointer::integer(value)
        }
    }

    pub fn allocate_u64(
        &self,
        value: u64,
        prototype: ObjectPointer,
    ) -> ObjectPointer {
        if ObjectPointer::unsigned_integer_as_big_integer(value) {
            // The value is too large to fit in a i64, so we need to allocate it
            // as a big integer.
            self.allocate(object_value::bigint(BigInt::from(value)), prototype)
        } else if ObjectPointer::unsigned_integer_too_large(value) {
            // The value is small enough that it can fit in an i64, but not
            // small enough that it can fit in a _tagged_ i64.
            self.allocate(object_value::integer(value as i64), prototype)
        } else {
            ObjectPointer::integer(value as i64)
        }
    }

    pub fn allocate_f64_as_i64(
        &self,
        value: f64,
        prototype: ObjectPointer,
    ) -> Result<ObjectPointer, String> {
        if value.is_nan() {
            return Err("A NaN can not be converted to an Integer".to_string());
        } else if value.is_infinite() {
            return Err("An infinite Float can not be converted to an Integer"
                .to_string());
        }

        // We use >= and <= here, as i64::MAX as a f64 can't be casted back to
        // i64, since `i64::MAX as f64` will produce a value slightly larger
        // than `i64::MAX`.
        let pointer = if value >= i64::MAX as f64 || value <= i64::MIN as f64 {
            self.allocate(
                object_value::bigint(BigInt::from_f64(value).unwrap()),
                prototype,
            )
        } else {
            self.allocate_i64(value as i64, prototype)
        };

        Ok(pointer)
    }

    pub fn allocate(
        &self,
        value: object_value::ObjectValue,
        proto: ObjectPointer,
    ) -> ObjectPointer {
        let local_data = self.local_data_mut();

        local_data.allocator.allocate_with_prototype(value, proto)
    }

    pub fn allocate_without_prototype(
        &self,
        value: object_value::ObjectValue,
    ) -> ObjectPointer {
        let local_data = self.local_data_mut();

        local_data.allocator.allocate_without_prototype(value)
    }

    pub fn send_message_from_external_process(
        &self,
        sender: RcProcess,
        method: ObjectPointer,
        mut arguments: Chunk<ObjectPointer>,
    ) -> Result<(FutureConsumer, RescheduleRights), RuntimeError> {
        // The lock must be acquired first, otherwise we may end up copying data
        // during garbage collection.
        let mut shared_data = self.shared_data.lock();
        let local_data = self.local_data_mut();
        let (producer, consumer) = Future::new(sender);

        if shared_data.status.is_terminated() {
            return Ok((consumer, RescheduleRights::Failed));
        }

        // Update the arguments in-place with their copies. Updating in-place
        // saves us an extra Chunk allocation.
        for index in 0..arguments.len() {
            arguments[index] =
                local_data.allocator.copy_object(arguments[index])?;
        }

        let message = Message::new(method, arguments, producer);
        let resched = shared_data.try_reschedule_for_message();

        shared_data.mailbox.send(message);
        Ok((consumer, resched))
    }

    pub fn send_message_from_self(
        &self,
        sender: RcProcess,
        method: ObjectPointer,
        arguments: Chunk<ObjectPointer>,
    ) -> FutureConsumer {
        let mut shared_data = self.shared_data.lock();
        let (producer, consumer) = Future::new(sender);
        let message = Message::new(method, arguments, producer);

        shared_data.mailbox.send(message);
        consumer
    }

    pub fn schedule_next_message(&self) -> Result<bool, String> {
        let mut shared_data = self.shared_data.lock();
        let local_data = self.local_data_mut();

        if let Some(message) = shared_data.mailbox.receive() {
            let block = message.method.block_value()?;
            let mut context = ExecutionContext::from_block_with_receiver(
                &block,
                local_data.self_object,
            );

            for index in 0..message.arguments.len() {
                context.set_local(index as u16, message.arguments[index]);
            }

            local_data.generator = Some(Generator::running(Box::new(context)));
            local_data.future = Some(message.future);

            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn context(&self) -> &ExecutionContext {
        self.local_data()
            .generator
            .as_ref()
            .expect("Can't get an ExecutionContext without a generator")
            .context()
    }

    #[cfg_attr(feature = "cargo-clippy", allow(mut_from_ref))]
    pub fn context_mut(&self) -> &mut ExecutionContext {
        self.local_data_mut()
            .generator
            .as_mut()
            .expect("Can't get an ExecutionContext without a generator")
            .context_mut()
    }

    pub fn has_messages(&self) -> bool {
        !self.shared_data.lock().mailbox.messages.is_empty()
    }

    pub fn should_collect_young_generation(&self) -> bool {
        self.local_data().allocator.should_collect_young()
    }

    pub fn should_collect_mature_generation(&self) -> bool {
        self.local_data().allocator.should_collect_mature()
    }

    pub fn contexts(&self) -> Vec<&ExecutionContext> {
        self.local_data()
            .generator
            .as_ref()
            .expect("Can't get an ExecutionContext without a generator")
            .contexts()
    }

    /// Write barrier for tracking cross generation writes.
    ///
    /// This barrier is based on the Steele write barrier and tracks the object
    /// that is *written to*, not the object that is being written.
    pub fn write_barrier(
        &self,
        written_to: ObjectPointer,
        written: ObjectPointer,
    ) {
        if written_to.is_mature() && written.is_young() {
            self.local_data_mut().allocator.remember_object(written_to);
        }
    }

    pub fn prepare_for_collection(&self, mature: bool) -> bool {
        self.local_data_mut()
            .allocator
            .prepare_for_collection(mature)
    }

    pub fn reclaim_blocks(&self, state: &State, mature: bool) {
        self.local_data_mut()
            .allocator
            .reclaim_blocks(state, mature);
    }

    pub fn reclaim_all_blocks(&self) -> BlockList {
        let local_data = self.local_data_mut();
        let mut blocks = BlockList::new();

        for bucket in &mut local_data.allocator.young_generation {
            blocks.append(&mut bucket.blocks);
        }

        blocks.append(&mut local_data.allocator.mature_generation.blocks);

        blocks
    }

    pub fn terminate(&self) {
        // The shared data lock _must_ be acquired first, otherwise we may end
        // up reclaiming blocks while another process is allocating message into
        // them.
        let mut shared = self.shared_data.lock();

        // Once terminated we don't want to receive any messages any more, as
        // they will never be received and thus lead to an increase in memory.
        // Thus, we mark the process as terminated. We must do this _after_
        // acquiring the lock to ensure other processes sending messages will
        // observe the right value.
        shared.status.set_terminated();
        self.release_all_blocks();
    }

    pub fn set_self_object(&self, object: ObjectPointer) {
        self.local_data_mut().self_object = object;
    }

    pub fn each_global_pointer<F>(&self, mut callback: F)
    where
        F: FnMut(ObjectPointerPointer),
    {
        let local_data = self.local_data();

        if let Some(gen) = local_data.generator.as_ref() {
            if let Some(ptr) = gen.result_pointer_pointer() {
                callback(ptr);
            }
        }

        if !local_data.self_object.is_null() {
            callback(local_data.self_object.pointer());
        }
    }

    pub fn each_remembered_pointer<F>(&self, callback: F)
    where
        F: FnMut(ObjectPointerPointer),
    {
        self.local_data_mut()
            .allocator
            .each_remembered_pointer(callback);
    }

    pub fn prune_remembered_set(&self) {
        self.local_data_mut().allocator.prune_remembered_objects();
    }

    pub fn remember_object(&self, pointer: ObjectPointer) {
        self.local_data_mut().allocator.remember_object(pointer);
    }

    pub fn waiting_for_message(&self) {
        self.shared_data.lock().status.set_waiting_for_message(true);
    }

    pub fn no_longer_waiting_for_message(&self) {
        self.shared_data
            .lock()
            .status
            .set_waiting_for_message(false);
    }

    pub fn is_waiting_for_message(&self) -> bool {
        self.shared_data.lock().status.is_waiting_for_message()
    }

    pub fn set_result(&self, result: ObjectPointer) {
        self.local_data_mut()
            .generator
            .as_mut()
            .expect("Can't set the result without a generator")
            .set_result(result);
    }

    pub fn take_result(&self) -> Option<ObjectPointer> {
        self.local_data()
            .generator
            .as_ref()
            .expect("Can't take the result without a generator")
            .take_result()
    }

    pub fn take_future(&self) -> Option<FutureProducer> {
        self.local_data_mut().future.take()
    }

    fn release_all_blocks(&self) {
        let mut blocks = self.reclaim_all_blocks();

        for block in blocks.iter_mut() {
            block.reset();
            block.finalize();
        }

        self.local_data()
            .allocator
            .global_allocator
            .add_blocks(&mut blocks);
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.release_all_blocks();
    }
}

impl RcProcess {
    /// Returns the unique identifier associated with this process.
    pub fn identifier(&self) -> usize {
        self.as_ptr() as usize
    }
}

impl PartialEq for RcProcess {
    fn eq(&self, other: &Self) -> bool {
        self.identifier() == other.identifier()
    }
}

impl Eq for RcProcess {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object_pointer::ObjectPointer;
    use crate::vm::test::setup;
    use num_bigint::BigInt;
    use std::f64;
    use std::i32;
    use std::i64;
    use std::mem;

    #[test]
    fn test_contexts() {
        let (_machine, _block, process) = setup();

        assert_eq!(process.contexts().len(), 1);
    }

    #[test]
    fn test_reclaim_blocks_without_mature() {
        let (machine, _block, process) = setup();

        {
            let local_data = process.local_data_mut();

            local_data.allocator.young_config.increment_allocations();
            local_data.allocator.mature_config.increment_allocations();
        }

        process.reclaim_blocks(&machine.state, false);

        let local_data = process.local_data();

        assert_eq!(local_data.allocator.young_config.block_allocations, 0);
        assert_eq!(local_data.allocator.mature_config.block_allocations, 1);
    }

    #[test]
    fn test_reclaim_blocks_with_mature() {
        let (machine, _block, process) = setup();

        {
            let local_data = process.local_data_mut();

            local_data.allocator.young_config.increment_allocations();
            local_data.allocator.mature_config.increment_allocations();
        }

        process.reclaim_blocks(&machine.state, true);

        let local_data = process.local_data();

        assert_eq!(local_data.allocator.young_config.block_allocations, 0);
        assert_eq!(local_data.allocator.mature_config.block_allocations, 0);
    }

    #[test]
    fn test_allocate_f64_as_i64_with_a_small_float() {
        let (machine, _block, process) = setup();

        let result =
            process.allocate_f64_as_i64(1.5, machine.state.integer_prototype);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().integer_value().unwrap(), 1);
    }

    #[test]
    fn test_allocate_f64_as_i64_with_a_medium_float() {
        let (machine, _block, process) = setup();

        let float = i32::MAX as f64;
        let result =
            process.allocate_f64_as_i64(float, machine.state.integer_prototype);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().integer_value().unwrap(), i32::MAX as i64);
    }

    #[test]
    fn test_allocate_f64_as_i64_with_a_large_float() {
        let (machine, _block, process) = setup();

        let float = i64::MAX as f64;
        let result =
            process.allocate_f64_as_i64(float, machine.state.integer_prototype);

        let max = BigInt::from(i64::MAX);

        assert!(result.is_ok());
        assert!(result.unwrap().bigint_value().unwrap() >= &max);
    }

    #[test]
    fn test_allocate_f64_as_i64_with_a_nan() {
        let (machine, _block, process) = setup();
        let result = process
            .allocate_f64_as_i64(f64::NAN, machine.state.integer_prototype);

        assert!(result.is_err());
    }

    #[test]
    fn test_allocate_f64_as_i64_with_infinity() {
        let (machine, _block, process) = setup();
        let result = process.allocate_f64_as_i64(
            f64::INFINITY,
            machine.state.integer_prototype,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_allocate_f64_as_i64_with_negative_infinity() {
        let (machine, _block, process) = setup();
        let result = process.allocate_f64_as_i64(
            f64::NEG_INFINITY,
            machine.state.integer_prototype,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_type_sizes() {
        // This test is put in place to ensure the type size doesn't change
        // unintentionally.
        assert_eq!(mem::size_of::<Process>(), 376);
        assert_eq!(mem::size_of::<Message>(), 32);
        assert_eq!(mem::size_of::<Future>(), 24);
    }

    #[test]
    fn test_process_set_thread_id() {
        let (_machine, _block, process) = setup();

        assert!(process.thread_id().is_none());

        process.set_thread_id(4);

        assert_eq!(process.thread_id(), Some(4));

        process.unset_thread_id();

        assert!(process.thread_id().is_none());
    }

    #[test]
    fn test_identifier() {
        let (_machine, _block, process) = setup();

        assert!(process.identifier() > 0);
    }

    #[test]
    fn test_each_global_pointer() {
        let (_machine, _block, process) = setup();

        process.set_result(ObjectPointer::integer(7));

        let mut pointers = Vec::new();

        process.each_global_pointer(|ptr| pointers.push(ptr));

        assert_eq!(pointers.len(), 3);
    }

    #[test]
    fn test_status_new_status() {
        let status = ProcessStatus::new();

        assert_eq!(status.is_main(), false);
        assert_eq!(status.is_blocking(), false);
        assert_eq!(status.is_terminated(), false);
    }

    #[test]
    fn test_status_set_main() {
        let mut status = ProcessStatus::new();

        assert_eq!(status.is_main(), false);

        status.set_main();

        assert!(status.is_main());
    }

    #[test]
    fn test_status_set_blocking() {
        let mut status = ProcessStatus::new();

        assert_eq!(status.is_blocking(), false);

        status.set_blocking(true);

        assert!(status.is_blocking());

        status.set_blocking(false);

        assert_eq!(status.is_blocking(), false);
    }

    #[test]
    fn test_status_set_multiple() {
        let mut status = ProcessStatus::new();

        status.set_main();
        status.set_blocking(true);
        status.set_waiting_for_message(true);

        assert!(status.is_main());
        assert!(status.is_blocking());
        assert!(status.is_waiting_for_message());
    }

    #[test]
    fn test_status_set_terminated() {
        let mut status = ProcessStatus::new();

        assert_eq!(status.is_terminated(), false);

        status.set_terminated();

        assert!(status.is_terminated());
    }

    #[test]
    fn test_status_is_waiting() {
        let mut status = ProcessStatus::new();

        status.set_waiting_for_message(true);

        assert!(status.is_waiting());

        status.set_waiting_for_message(false);
        status.set_waiting_for_future(true);

        assert!(status.is_waiting());

        status.set_waiting_for_message(true);

        assert!(status.is_waiting());

        status.no_longer_waiting();

        assert_eq!(status.is_waiting(), false);
    }

    #[test]
    fn test_mailbox_send_receive() {
        let (_machine, _block, process) = setup();
        let mut mailbox = Mailbox::new();
        let (prod, _) = Future::new(process);

        mailbox.send(Message::new(ObjectPointer::null(), Chunk::new(0), prod));

        assert!(mailbox.receive().is_some());
    }

    #[test]
    fn test_mailbox_each_pointer() {
        let (_machine, _block, process) = setup();
        let mut mailbox = Mailbox::new();
        let (prod, _) = Future::new(process);
        let mut args = Chunk::new(1);

        args[0] = ObjectPointer::new(0x1 as _);

        mailbox.send(Message::new(ObjectPointer::null(), args, prod));

        let mut pointers = Vec::new();

        mailbox.each_pointer(|ptr| pointers.push(ptr));

        while let Some(ptr) = pointers.pop() {
            ptr.get_mut().raw.raw = 0x4 as _;
        }

        let received = mailbox.receive().unwrap();

        assert!(received.arguments[0] == ObjectPointer::new(0x4 as _));
    }

    #[test]
    fn test_future_new() {
        let (_machine, _block, process) = setup();
        let (_, consumer) = Future::new(process);
        let lock = consumer.lock();

        assert_eq!(lock.disconnected, false);
        assert_eq!(lock.waiting, false);
        assert!(lock.result.is_null());
    }

    #[test]
    fn test_future_waiting() {
        let (_machine, _block, process) = setup();
        let (_, consumer) = Future::new(process);
        let mut lock = consumer.lock();

        lock.waiting();

        assert!(lock.waiting);

        lock.no_longer_waiting();

        assert_eq!(lock.waiting, false);
    }

    #[test]
    fn test_future_waiting_with_result() {
        let (_machine, _block, process) = setup();
        let (_, consumer) = Future::new(process);
        let mut lock = consumer.lock();

        lock.result = ObjectPointer::new(0x4 as _);

        lock.waiting();

        assert_eq!(lock.waiting, false);
    }

    #[test]
    fn test_future_result() {
        let (_machine, _block, process) = setup();
        let (_, consumer) = Future::new(process);
        let mut lock = consumer.lock();

        assert!(lock.result().is_none());

        lock.result = ObjectPointer::new(0x4 as _);

        assert!(lock.result().is_some());
    }

    #[test]
    fn test_future_result_pointer_pointer() {
        let (_machine, _block, process) = setup();
        let (_, consumer) = Future::new(process);
        let mut lock = consumer.lock();

        lock.result = ObjectPointer::new(0x4 as _);

        assert_eq!(
            lock.result_pointer_pointer().unwrap().get().raw.raw as usize,
            0x4
        );
    }

    #[test]
    fn test_future_consumer_drop() {
        let (_machine, _block, process) = setup();
        let (producer, consumer) = Future::new(process);

        drop(consumer);

        assert!(producer.future.lock().disconnected);
    }

    #[test]
    fn test_future_producer_set_result_disconnected() {
        let (_machine, _block, process) = setup();
        let (producer, consumer) = Future::new(process.clone());
        let res_ptr = ObjectPointer::integer(42);

        drop(consumer);

        let result = producer.set_result(&process, res_ptr, false);

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
        assert!(producer.future.lock().result.is_null());
    }

    #[test]
    fn test_future_producer_set_result_from_same_process() {
        let (_machine, _block, process) = setup();
        let (producer, _consumer) = Future::new(process.clone());
        let res_ptr = ObjectPointer::integer(42);
        let result = producer.set_result(&process, res_ptr, false);

        assert!(result.is_ok());
        assert_eq!(producer.future.lock().result.integer_value().unwrap(), 42);
    }

    #[test]
    fn test_future_producer_set_result_thrown() {
        let (_machine, _block, process) = setup();
        let (producer, _consumer) = Future::new(process.clone());
        let res_ptr = ObjectPointer::integer(42);
        let result = producer.set_result(&process, res_ptr, true);

        assert!(result.is_ok());
        assert!(producer.future.lock().thrown);
    }

    #[test]
    fn test_future_producer_set_result_from_different_process() {
        let (machine, _block, process) = setup();
        let (producer, _consumer) = Future::new(process.clone());
        let sender = Process::new(
            machine.state.global_allocator.clone(),
            &machine.state.config,
        );

        let res_ptr =
            process.allocate_without_prototype(object_value::float(1.2));
        let result = producer.set_result(&sender, res_ptr, false);

        assert!(result.is_ok());
        assert!(result.unwrap() == Some((RescheduleRights::Failed, process)));

        // The data is copied, so the pointer is different.
        assert_eq!(producer.future.lock().result.float_value().unwrap(), 1.2);
    }

    #[test]
    fn test_future_producer_set_result_from_different_process_with_waiting() {
        let (machine, _block, process) = setup();
        let (producer, _consumer) = Future::new(process.clone());
        let sender = Process::new(
            machine.state.global_allocator.clone(),
            &machine.state.config,
        );

        process.shared_data().park_for_future(None);

        let res_ptr =
            process.allocate_without_prototype(object_value::float(1.2));
        let result = producer.set_result(&sender, res_ptr, false);

        assert!(result.is_ok());
        assert!(result.unwrap() == Some((RescheduleRights::Acquired, process)));

        // The data is copied, so the pointer is different.
        assert_eq!(producer.future.lock().result.float_value().unwrap(), 1.2);
    }
}
