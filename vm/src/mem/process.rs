//! Data structures for Inko processes and futures.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::execution_context::ExecutionContext;
use crate::indexes::{FieldIndex, MethodIndex};
use crate::mem::allocator::{Allocator, Pointer};
use crate::mem::generator::{Generator, GeneratorPointer};
use crate::mem::objects::{ClassPointer, Header, MethodPointer};
use crate::scheduler::timeouts::Timeout;
use parking_lot::{Mutex, MutexGuard};
use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::collections::VecDeque;
use std::mem::{align_of, size_of};
use std::ops::Drop;
use std::ops::{Deref, DerefMut};
use std::ptr::{copy_nonoverlapping, drop_in_place, NonNull};
use std::slice;

/// The shared state of a future.
pub struct FutureState {
    /// The process that's waiting for this future to complete.
    consumer: Option<ServerPointer>,

    /// The result of a message.
    result: Option<Pointer>,

    /// A boolean indicating that the result was produced by throwing an error,
    /// instead of returning it.
    thrown: bool,

    /// A boolean indicating the future is disconnected.
    ///
    /// This flag is set whenever a reader or writer is dropped.
    disconnected: bool,
}

impl FutureState {
    pub fn new() -> ArcWithoutWeak<Mutex<FutureState>> {
        let state = Self {
            consumer: None,
            result: None,
            thrown: false,
            disconnected: false,
        };

        ArcWithoutWeak::new(Mutex::new(state))
    }
}

/// A message scheduled for execution at some point in the future.
///
/// The size of this type depends on the number of arguments. Using this
/// approach allows us to keep the size of a message as small as possible,
/// without the need for allocating arguments separately.
#[repr(C)]
pub struct MessageInner {
    /// The method to run.
    method: MethodPointer,

    /// The number of arguments of this message.
    length: u8,

    /// The arguments of the message.
    arguments: [Pointer; 0],
}

/// An owned pointer to a message.
pub struct Message(NonNull<MessageInner>);

impl Message {
    pub fn new(method: MethodPointer, arguments: &[Pointer]) -> Self {
        unsafe {
            let layout = Self::layout(arguments.len() as u8);
            let raw_ptr = alloc(layout) as *mut MessageInner;

            if raw_ptr.is_null() {
                handle_alloc_error(layout);
            }

            let msg = &mut *raw_ptr;

            init!(msg.method => method);
            init!(msg.length => arguments.len() as u8);

            copy_nonoverlapping(
                arguments.as_ptr(),
                msg.arguments.as_mut_ptr(),
                arguments.len(),
            );

            Self(NonNull::new_unchecked(raw_ptr))
        }
    }

    unsafe fn layout(length: u8) -> Layout {
        let size = size_of::<MessageInner>()
            + (length as usize * size_of::<Pointer>());

        // Messages are sent often, so we don't want the overhead of size and
        // alignment checks.
        Layout::from_size_align_unchecked(size, align_of::<Self>())
    }
}

impl Deref for Message {
    type Target = MessageInner;

    fn deref(&self) -> &MessageInner {
        unsafe { self.0.as_ref() }
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        unsafe {
            let layout = Self::layout(self.0.as_ref().length);

            drop_in_place(self.0.as_ptr());
            dealloc(self.0.as_ptr() as *mut u8, layout);
        }
    }
}

/// A collection of messages to be processed by a process.
pub struct Mailbox {
    messages: VecDeque<Message>,
}

impl Mailbox {
    pub fn new() -> Self {
        Mailbox {
            messages: VecDeque::new(),
        }
    }

    fn send(&mut self, message: Message) {
        self.messages.push_back(message);
    }

    fn receive(&mut self) -> Option<Message> {
        self.messages.pop_front()
    }
}

/// The status of a process, represented as a set of bits.
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

    /// The process is waiting for a message.
    const WAITING_FOR_MESSAGE: u8 = 0b100;

    /// The process is waiting for a future.
    const WAITING_FOR_FUTURE: u8 = 0b1000;

    /// The process is simply sleeping for a certain amount of time.
    const SLEEPING: u8 = 0b1_0000;

    /// The process was rescheduled after a timeout expired.
    const TIMEOUT_EXPIRED: u8 = 0b10_0000;

    /// The process is waiting for something, or suspended for a period of time
    const WAITING: u8 = Self::WAITING_FOR_FUTURE | Self::SLEEPING;

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
#[derive(Eq, PartialEq, Debug)]
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
        !matches!(self, RescheduleRights::Failed)
    }
}

/// An enum describing what to do when finishing a generator.
#[derive(PartialEq, Eq, Debug)]
pub enum Finished {
    /// There's more work to process, so the process needs to be rescheduled.
    Reschedule,

    /// There are no messages to process, but there are still clients that may
    /// produce new messages.
    WaitForMessage,

    /// There are no messages left, and no clients exist.
    Terminate,
}

/// The shared state of a process.
///
/// This state is shared by both the server and its clients.
pub struct ProcessState {
    /// The mailbox of this process.
    mailbox: Mailbox,

    /// The status of the process.
    status: ProcessStatus,

    /// The number of clients connected to this process.
    clients: usize,

    /// The timeout this process is suspended with, if any.
    ///
    /// If missing and the process is suspended, it means the process is
    /// suspended indefinitely.
    timeout: Option<ArcWithoutWeak<Timeout>>,
}

impl ProcessState {
    pub fn new() -> Self {
        Self {
            mailbox: Mailbox::new(),
            status: ProcessStatus::new(),
            clients: 0,
            timeout: None,
        }
    }

    pub fn has_same_timeout(&self, timeout: &ArcWithoutWeak<Timeout>) -> bool {
        self.timeout
            .as_ref()
            .map(|t| t.as_ptr() == timeout.as_ptr())
            .unwrap_or(false)
    }

    /// Tries to acquire the rescheduling rights after a timeout expired.
    pub fn try_reschedule_after_timeout(&mut self) -> RescheduleRights {
        if !self.status.is_waiting() {
            return RescheduleRights::Failed;
        }

        if self.status.is_waiting_for_future() {
            // If we were waiting for a future, it means the timeout has
            // expired.
            self.status.set_timeout_expired(true);
        }

        self.status.no_longer_waiting();

        if self.timeout.take().is_some() {
            RescheduleRights::AcquiredWithTimeout
        } else {
            RescheduleRights::Acquired
        }
    }

    pub fn waiting_for_future(
        &mut self,
        timeout: Option<ArcWithoutWeak<Timeout>>,
    ) {
        self.timeout = timeout;

        self.status.set_waiting_for_future(true);
    }

    /// Tries to acquire the rescheduling rights for when a message is sent to a
    /// process.
    fn try_reschedule_for_message(&mut self) -> RescheduleRights {
        if !self.status.is_waiting_for_message() {
            return RescheduleRights::Failed;
        }

        self.status.set_waiting_for_message(false);
        RescheduleRights::Acquired
    }

    /// Tries to acquire the rescheduling rights for when a value is written to
    /// a future.
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

/// The server side of a process.
#[repr(C)]
pub struct Server {
    header: Header,

    /// The ID of the thread this server is pinned to.
    thread_id: Option<u16>,

    /// The currently running generator, if any.
    generator: Option<GeneratorPointer>,

    /// The internal state of the process.
    state: ArcWithoutWeak<Mutex<ProcessState>>,

    /// The fields of this process.
    ///
    /// The length of this flexible array is derived from the number of
    /// fields defined in this process' class.
    fields: [Pointer; 0],
}

impl Server {
    /// Drops the given Server.
    pub fn drop(ptr: ServerPointer) {
        unsafe {
            drop_in_place(ptr.as_pointer().as_ptr() as *mut Self);
        }
    }

    /// Returns a new Process acting as the server side.
    pub fn alloc<A: Allocator>(
        alloc: &mut A,
        class: ClassPointer,
    ) -> ServerPointer {
        let ptr = alloc.allocate(class.instance_size());
        let obj = unsafe { ptr.get_mut::<Self>() };
        let state = ArcWithoutWeak::new(Mutex::new(ProcessState::new()));

        obj.header.init(class);

        init!(obj.state => state);
        init!(obj.generator => None);
        init!(obj.thread_id => None);

        unsafe { ServerPointer::new(ptr) }
    }

    /// Returns a new Server acting as the main process/server.
    ///
    /// The main process is also pinned to the main thread. This ensures the
    /// main process always runs on a predictable thread.
    pub fn main<A: Allocator>(
        alloc: &mut A,
        process_class: ClassPointer,
        generator_class: ClassPointer,
        method: MethodPointer,
    ) -> ServerPointer {
        let mut server = Self::alloc(alloc, process_class);

        server.set_main();
        server.pin_to_thread(0);
        server.schedule_module_method(alloc, generator_class, method);
        server
    }

    pub fn set_main(&mut self) {
        self.state.lock().status.set_main();
    }

    pub fn is_main(&self) -> bool {
        self.state.lock().status.is_main()
    }

    pub fn thread_id(&self) -> Option<u16> {
        self.thread_id
    }

    pub fn pin_to_thread(&mut self, thread: u16) {
        self.thread_id = Some(thread);
    }

    pub fn unpin_from_thread(&mut self) {
        self.thread_id = None;
    }

    pub fn set_blocking(&mut self) {
        self.state.lock().status.set_blocking(true);
    }

    pub fn no_longer_blocking(&mut self) {
        self.state.lock().status.set_blocking(false);
    }

    pub fn is_blocking(&self) -> bool {
        self.state.lock().status.is_blocking()
    }

    pub fn resume_generator(&mut self, mut generator: GeneratorPointer) {
        generator.parent = self.generator;
        self.generator = Some(generator);
    }

    /// Stops execution of the current generator.
    ///
    /// This method returns `true` if there are no more generators to run.
    pub fn pop_generator(&mut self) -> bool {
        if let Some(mut generator) = self.generator {
            self.generator = generator.parent.take();

            if self.generator.is_some() {
                false
            } else {
                // If the generator doesn't have a parent, it means it can't
                // exist on the stack (any generator created through user code
                // has a parent). In this case we must drop the generator.
                Generator::drop(generator);
                true
            }
        } else {
            true
        }
    }

    /// Suspends this process for a period of time.
    ///
    /// A process is sleeping when it simply isn't to be scheduled for a while,
    /// without waiting for a message, future, or something else.
    pub fn suspend(&mut self, timeout: ArcWithoutWeak<Timeout>) {
        let mut state = self.state.lock();

        state.timeout = Some(timeout);

        state.status.set_sleeping(true);
    }

    /// Schedules a module method (without arguments) for execution.
    ///
    /// This method is to be used when a process simply needs to run a module
    /// method, such as a module's `main` method.
    pub fn schedule_module_method<A: Allocator>(
        &mut self,
        alloc: &mut A,
        generator_class: ClassPointer,
        method: MethodPointer,
    ) {
        let ctx = ExecutionContext::for_method(method);
        let gen = Generator::alloc(alloc, generator_class, ctx);

        self.resume_generator(gen);
    }

    /// Sends a message to this process.
    ///
    /// The return value indicates what to do with the receiver after sending
    /// the message (e.g. rescheduling it).
    pub fn send_message(
        &mut self,
        method_index: MethodIndex,
        arguments: &[Pointer],
    ) -> RescheduleRights {
        let method = self.header.class.get_method(method_index);
        let message = Message::new(method, arguments);
        let mut state = self.state.lock();

        state.mailbox.send(message);
        state.try_reschedule_for_message()
    }

    /// Returns the generator to run next.
    ///
    /// If no generator is available to run, a None is returned and the process'
    /// status is changed to "waiting for message".
    pub fn generator_to_run<A: Allocator>(
        &mut self,
        alloc: &mut A,
        generator_class: ClassPointer,
    ) -> Option<GeneratorPointer> {
        let mut proc_state = self.state.lock();

        if self.generator.is_some() {
            return self.generator;
        }

        let message = if let Some(message) = proc_state.mailbox.receive() {
            message
        } else {
            proc_state.status.set_waiting_for_message(true);

            return None;
        };

        drop(proc_state);

        let mut ctx = ExecutionContext::for_method(message.method);
        let len = message.length as usize;
        let values =
            unsafe { slice::from_raw_parts(message.arguments.as_ptr(), len) };

        ctx.set_registers(values);

        let gen = Generator::alloc(alloc, generator_class, ctx);

        self.resume_generator(gen);

        Some(gen)
    }

    /// Finishes the exection of a generator, and decides what to do next with
    /// this process.
    pub fn finish_generator(&self) -> Finished {
        let mut state = self.state.lock();

        if !state.mailbox.messages.is_empty() {
            return Finished::Reschedule;
        }

        if state.clients > 0 {
            state.status.set_waiting_for_message(true);

            return Finished::WaitForMessage;
        }

        Finished::Terminate
    }

    /// Assigns new values to multiple fields.
    ///
    /// # Panics
    ///
    /// If the number of values to set differs from the number of fields, this
    /// method panics.
    pub fn set_values(&mut self, values: &[Pointer]) {
        let length = self.header.class.fields().len();

        unsafe {
            slice::from_raw_parts_mut(self.fields.as_mut_ptr(), length)
                .copy_from_slice(values);
        }
    }

    /// Assigns a new value to a field.
    pub fn set(&mut self, index: FieldIndex, value: Pointer) {
        unsafe { self.fields.as_mut_ptr().add(index.into()).write(value) };
    }

    /// Returns the value of a field.
    pub fn get(&self, index: FieldIndex) -> Pointer {
        unsafe { *self.fields.as_ptr().add(index.into()) }
    }

    /// Returns `true` if the timeout of this process expired, clearing the
    /// expiration status.
    pub fn timeout_expired(&self) -> bool {
        let mut state = self.state.lock();

        if state.status.timeout_expired() {
            state.status.set_timeout_expired(false);
            true
        } else {
            false
        }
    }

    /// Locks and returns a reference to the process state.
    pub fn state(&self) -> MutexGuard<ProcessState> {
        self.state.lock()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(gen) = self.generator {
            Generator::drop(gen);
        }
    }
}

/// A pointer to the server side of a process.
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ServerPointer(Pointer);

impl ServerPointer {
    /// Returns a new ProcessServerPointer from a raw Pointer.
    ///
    /// This method is unsafe as it doesn't perform any checks to ensure the raw
    /// pointer actually points to a process.
    pub unsafe fn new(pointer: Pointer) -> Self {
        Self(pointer)
    }

    /// Returns the unique identifier of the underlying process.
    pub fn identifier(self) -> usize {
        self.0.as_ptr() as usize
    }

    /// Returns a regular Pointer to the process.
    pub fn as_pointer(self) -> Pointer {
        self.0
    }
}

impl Deref for ServerPointer {
    type Target = Server;

    fn deref(&self) -> &Server {
        unsafe { self.0.get::<Server>() }
    }
}

impl DerefMut for ServerPointer {
    fn deref_mut(&mut self) -> &mut Server {
        unsafe { self.0.get_mut::<Server>() }
    }
}

/// The client side of a process.
#[repr(C)]
pub struct Client {
    header: Header,

    /// The server we are a client of.
    ///
    /// We track the entire server here and not just its state, allowing clients
    /// to reschedule servers whenever necessary.
    server: ServerPointer,
}

impl Client {
    /// Returns a new Process acting as the client side.
    pub fn alloc<A: Allocator>(
        alloc: &mut A,
        class: ClassPointer,
        server: ServerPointer,
    ) -> ClientPointer {
        let ptr = alloc.allocate(size_of::<Self>());
        let obj = unsafe { ptr.get_mut::<Self>() };

        server.state.lock().clients += 1;

        obj.header.init(class);

        init!(obj.server => server);

        unsafe { ClientPointer::new(ptr) }
    }

    /// Drops a client.
    pub fn drop(ptr: ClientPointer) {
        unsafe {
            drop_in_place(ptr.as_pointer().as_ptr() as *mut Self);
        }
    }

    /// Returns the pointer to the server this client is connected to.
    pub fn server(&self) -> ServerPointer {
        self.server
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.server.state.lock().clients -= 1;
    }
}

/// A pointer to the client side of a process.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct ClientPointer(Pointer);

impl ClientPointer {
    /// Returns a new ClientPointer from a raw Pointer.
    ///
    /// This method is unsafe as it doesn't perform any checks to ensure the raw
    /// pointer actually points to a process.
    pub unsafe fn new(pointer: Pointer) -> Self {
        Self(pointer)
    }

    pub fn as_pointer(self) -> Pointer {
        self.0
    }
}

impl Deref for ClientPointer {
    type Target = Client;

    fn deref(&self) -> &Client {
        unsafe { self.0.get::<Client>() }
    }
}

/// An enum describing the value produced by a future, and how it was produced.
#[derive(PartialEq, Eq, Debug)]
pub enum FutureResult {
    /// No values has been produced so far.
    None,

    /// The value was returned.
    Returned(Pointer),

    /// The value was thrown and should be thrown again.
    Thrown(Pointer),
}

/// An enum that describes what a producer should do after writing to a future.
#[derive(PartialEq, Eq, Debug)]
pub enum WriteResult {
    /// The future is disconnected, and the writer should discard the value it
    /// tried to write.
    Discard,

    /// A value was written, but no further action is needed.
    Continue,

    /// A value was written, and the given process needs to be rescheduled.
    Reschedule(ServerPointer),

    /// A value was written, the given process needs to be rescheduled, and a
    /// timeout needs to be invalidated.
    RescheduleWithTimeout(ServerPointer),
}

/// Storage for a value to be computed some time in the future.
///
/// A Future is essentially just a single-producer single-consumer queue, with
/// support for only writing a value once, and only reading it once. Futures are
/// used to send message results between processes.
#[repr(C)]
pub struct Future {
    header: Header,
    state: ArcWithoutWeak<Mutex<FutureState>>,
}

impl Future {
    /// Returns a new Future.
    pub fn alloc<A: Allocator>(
        alloc: &mut A,
        class: ClassPointer,
        state: ArcWithoutWeak<Mutex<FutureState>>,
    ) -> Pointer {
        let ptr = alloc.allocate(size_of::<Self>());
        let obj = unsafe { ptr.get_mut::<Self>() };

        obj.header.init(class);

        init!(obj.state => state);

        ptr
    }

    /// Drops a future.
    pub unsafe fn drop(ptr: Pointer) {
        drop_in_place(ptr.untagged_ptr() as *mut Self);
    }

    /// Returns two futures: one for reading values, and one for writing values.
    pub fn reader_writer<A: Allocator>(
        alloc: &mut A,
        class: ClassPointer,
    ) -> (Pointer, Pointer) {
        let state1 = FutureState::new();
        let state2 = state1.clone();

        (
            Self::alloc(alloc, class, state1),
            Self::alloc(alloc, class, state2),
        )
    }

    /// Writes the result of a message to this future.
    ///
    /// If the future has been disconnected, an Err is returned that contains
    /// the value that we tried to write.
    ///
    /// The returned tuple contains a value that indicates if a process needs to
    /// be rescheduled, and a pointer to the server side of the process that
    /// sent the message.
    pub fn write(&mut self, result: Pointer, thrown: bool) -> WriteResult {
        let mut future = self.state.lock();

        if future.disconnected {
            return WriteResult::Discard;
        }

        future.thrown = thrown;
        future.result = Some(result);

        if let Some(consumer) = future.consumer.take() {
            match consumer.state.lock().try_reschedule_for_future() {
                RescheduleRights::Failed => WriteResult::Continue,
                RescheduleRights::Acquired => WriteResult::Reschedule(consumer),
                RescheduleRights::AcquiredWithTimeout => {
                    WriteResult::RescheduleWithTimeout(consumer)
                }
            }
        } else {
            WriteResult::Continue
        }
    }

    /// Gets the value from this future.
    ///
    /// When a None is returned, the consumer must stop running any code. It's
    /// then up to a producer to reschedule the consumer when writing a value to
    /// the future.
    pub fn get(
        &self,
        consumer: ServerPointer,
        timeout: Option<ArcWithoutWeak<Timeout>>,
    ) -> FutureResult {
        // The unlocking order is important here: we _must_ unlock the future
        // _before_ unlocking the consumer. If we unlock the consumer first, we
        // could deadlock with processes writing to this future.
        let mut future = self.state.lock();
        let mut state = consumer.state.lock();

        if let Some(result) = future.result.take() {
            future.consumer = None;

            state.status.set_waiting_for_future(false);

            return if future.thrown {
                FutureResult::Thrown(result)
            } else {
                FutureResult::Returned(result)
            };
        }

        future.consumer = Some(consumer);
        state.waiting_for_future(timeout);

        FutureResult::None
    }
}

impl Drop for Future {
    fn drop(&mut self) {
        // Futures come in pairs, consisting of a reader and writer. The shared
        // state is allocated separately. Setting the `disconnected` flag when
        // dropping one half allows the other half to check if the reader and
        // writer are disconnected.
        self.state.lock().disconnected = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::objects::{Class, Method, OwnedClass};
    use crate::test::{
        bump_allocator, empty_class, empty_method, empty_module,
        empty_process_class, OwnedServer,
    };
    use std::mem::size_of;
    use std::time::Duration;

    #[test]
    fn test_type_sizes() {
        assert_eq!(size_of::<FutureState>(), 24);
        assert_eq!(size_of::<Mutex<FutureState>>(), 32);
        assert_eq!(size_of::<Future>(), 24);
        assert_eq!(size_of::<Client>(), 24);
        assert_eq!(size_of::<Server>(), 40);
    }

    #[test]
    fn test_message_new() {
        let mut alloc = bump_allocator();
        let class = empty_class("A");
        let module = empty_module(&mut alloc, class);
        let method = empty_method(&mut alloc, module.class(), *module, "foo");
        let message = Message::new(method, &[Pointer::int(1), Pointer::int(2)]);

        assert_eq!(message.method.as_pointer(), method.as_pointer());
        assert_eq!(message.length, 2);

        unsafe {
            assert_eq!(*message.arguments.as_ptr().add(0), Pointer::int(1));
            assert_eq!(*message.arguments.as_ptr().add(1), Pointer::int(2));
        }

        Method::drop(method);
    }

    #[test]
    fn test_mailbox_send_receive() {
        let mut alloc = bump_allocator();
        let class = empty_class("A");
        let module = empty_module(&mut alloc, class);
        let method = empty_method(&mut alloc, module.class(), *module, "foo");
        let message = Message::new(method, &[Pointer::int(1), Pointer::int(2)]);
        let mut mailbox = Mailbox::new();

        mailbox.send(message);

        let message = mailbox.receive().unwrap();

        assert_eq!(message.length, 2);

        Method::drop(method);
    }

    #[test]
    fn test_process_status_new() {
        let status = ProcessStatus::new();

        assert_eq!(status.bits, 0);
    }

    #[test]
    fn test_process_status_set_main() {
        let mut status = ProcessStatus::new();

        status.set_main();

        assert!(status.is_main());
    }

    #[test]
    fn test_process_status_set_blocking() {
        let mut status = ProcessStatus::new();

        status.set_blocking(true);
        assert!(status.is_blocking());

        status.set_blocking(false);
        assert_eq!(status.is_blocking(), false);
    }

    #[test]
    fn test_process_status_set_waiting_for_message() {
        let mut status = ProcessStatus::new();

        status.set_waiting_for_message(true);
        assert!(status.is_waiting_for_message());

        status.set_waiting_for_message(false);
        assert_eq!(status.is_waiting_for_message(), false);
    }

    #[test]
    fn test_process_status_set_waiting_for_future() {
        let mut status = ProcessStatus::new();

        status.set_waiting_for_future(true);
        assert!(status.is_waiting_for_future());

        status.set_waiting_for_future(false);
        assert_eq!(status.is_waiting_for_future(), false);
    }

    #[test]
    fn test_process_status_is_waiting() {
        let mut status = ProcessStatus::new();

        status.set_sleeping(true);
        assert!(status.is_waiting());

        status.set_sleeping(false);
        status.set_waiting_for_future(true);
        assert!(status.is_waiting());

        status.no_longer_waiting();

        assert_eq!(status.is_waiting_for_future(), false);
        assert_eq!(status.is_waiting(), false);
    }

    #[test]
    fn test_process_status_timeout_expired() {
        let mut status = ProcessStatus::new();

        status.set_timeout_expired(true);
        assert!(status.timeout_expired());

        status.set_timeout_expired(false);
        assert_eq!(status.timeout_expired(), false);
    }

    #[test]
    fn test_reschedule_rights_are_acquired() {
        assert_eq!(RescheduleRights::Failed.are_acquired(), false);
        assert_eq!(RescheduleRights::Acquired.are_acquired(), true);
        assert_eq!(RescheduleRights::AcquiredWithTimeout.are_acquired(), true);
    }

    #[test]
    fn test_process_state_has_same_timeout() {
        let mut state = ProcessState::new();
        let timeout = Timeout::with_rc(Duration::from_secs(0));

        assert_eq!(state.has_same_timeout(&timeout), false);

        state.timeout = Some(timeout.clone());

        assert!(state.has_same_timeout(&timeout));
    }

    #[test]
    fn test_process_state_try_reschedule_after_timeout() {
        let mut state = ProcessState::new();

        assert_eq!(
            state.try_reschedule_after_timeout(),
            RescheduleRights::Failed
        );

        state.waiting_for_future(None);

        assert_eq!(
            state.try_reschedule_after_timeout(),
            RescheduleRights::Acquired
        );

        assert_eq!(state.status.is_waiting_for_future(), false);
        assert_eq!(state.status.is_waiting(), false);

        let timeout = Timeout::with_rc(Duration::from_secs(0));

        state.waiting_for_future(Some(timeout));

        assert_eq!(
            state.try_reschedule_after_timeout(),
            RescheduleRights::AcquiredWithTimeout
        );

        assert_eq!(state.status.is_waiting_for_future(), false);
        assert_eq!(state.status.is_waiting(), false);
    }

    #[test]
    fn test_process_state_waiting_for_future() {
        let mut state = ProcessState::new();
        let timeout = Timeout::with_rc(Duration::from_secs(0));

        state.waiting_for_future(None);

        assert!(state.status.is_waiting_for_future());
        assert!(state.timeout.is_none());

        state.waiting_for_future(Some(timeout));

        assert!(state.status.is_waiting_for_future());
        assert!(state.timeout.is_some());
    }

    #[test]
    fn test_process_state_try_reschedule_for_message() {
        let mut state = ProcessState::new();

        assert_eq!(
            state.try_reschedule_for_message(),
            RescheduleRights::Failed
        );

        state.status.set_waiting_for_message(true);

        assert_eq!(
            state.try_reschedule_for_message(),
            RescheduleRights::Acquired
        );
        assert_eq!(state.status.is_waiting_for_message(), false);
    }

    #[test]
    fn test_process_state_try_reschedule_for_future() {
        let mut state = ProcessState::new();

        assert_eq!(state.try_reschedule_for_future(), RescheduleRights::Failed);

        state.status.set_waiting_for_future(true);
        assert_eq!(
            state.try_reschedule_for_future(),
            RescheduleRights::Acquired
        );
        assert_eq!(state.status.is_waiting_for_future(), false);

        state.status.set_waiting_for_future(true);
        state.timeout = Some(Timeout::with_rc(Duration::from_secs(0)));

        assert_eq!(
            state.try_reschedule_for_future(),
            RescheduleRights::AcquiredWithTimeout
        );
        assert_eq!(state.status.is_waiting_for_future(), false);
    }

    #[test]
    fn test_server_new() {
        let mut alloc = bump_allocator();
        let class = empty_process_class("A");
        let server = OwnedServer::new(Server::alloc(&mut alloc, *class));

        assert_eq!(server.header.references, 0);
        assert_eq!(server.header.class.as_pointer(), class.as_pointer());
        assert_eq!(server.header.references, 0);
        assert!(server.generator.is_none());
        assert!(server.thread_id.is_none());
    }

    #[test]
    fn test_server_main() {
        let mut alloc = bump_allocator();
        let mod_class = empty_class("A");
        let proc_class = empty_process_class("A");
        let module = empty_module(&mut alloc, mod_class);
        let method = empty_method(&mut alloc, module.class(), *module, "foo");
        let server = OwnedServer::new(Server::main(
            &mut alloc,
            *proc_class,
            module.class(),
            method,
        ));

        assert!(server.is_main());
        assert_eq!(server.thread_id(), Some(0));

        Method::drop(method);
    }

    #[test]
    fn test_server_set_main() {
        let mut alloc = bump_allocator();
        let class = empty_process_class("A");
        let mut server = OwnedServer::new(Server::alloc(&mut alloc, *class));

        assert!(!server.is_main());

        server.set_main();
        assert!(server.is_main());
    }

    #[test]
    fn test_server_pin_thread() {
        let mut alloc = bump_allocator();
        let class = empty_process_class("A");
        let mut server = OwnedServer::new(Server::alloc(&mut alloc, *class));

        assert_eq!(server.thread_id(), None);

        server.pin_to_thread(4);

        assert_eq!(server.thread_id(), Some(4));

        server.unpin_from_thread();

        assert_eq!(server.thread_id(), None);
    }

    #[test]
    fn test_server_set_blocking() {
        let mut alloc = bump_allocator();
        let class = empty_process_class("A");
        let mut server = OwnedServer::new(Server::alloc(&mut alloc, *class));

        assert!(!server.is_blocking());

        server.set_blocking();
        assert!(server.is_blocking());

        server.no_longer_blocking();
        assert!(!server.is_blocking());
    }

    #[test]
    fn test_server_resume_and_pop_generator() {
        let mut alloc = bump_allocator();
        let mod_class = empty_class("A");
        let proc_class = empty_process_class("A");
        let module = empty_module(&mut alloc, mod_class);
        let mut server =
            OwnedServer::new(Server::alloc(&mut alloc, *proc_class));
        let method = empty_method(&mut alloc, module.class(), *module, "foo");
        let gen1 = Generator::alloc(
            &mut alloc,
            module.class(),
            ExecutionContext::for_method(method),
        );
        let gen2 = Generator::alloc(
            &mut alloc,
            module.class(),
            ExecutionContext::for_method(method),
        );

        server.resume_generator(gen1);
        server.resume_generator(gen2);

        assert!(server.generator.is_some());
        assert!(!server.pop_generator());
        assert!(server.pop_generator());
        assert!(server.generator.is_none());

        // gen1 is dropped when it's popped.
        Generator::drop(gen2);
        Method::drop(method);
    }

    #[test]
    fn test_server_suspend() {
        let mut alloc = bump_allocator();
        let class = empty_process_class("A");
        let mut server = OwnedServer::new(Server::alloc(&mut alloc, *class));
        let timeout = Timeout::with_rc(Duration::from_secs(0));

        server.suspend(timeout);

        assert!(server.state().timeout.is_some());
        assert!(server.state().status.is_waiting());
    }

    #[test]
    fn test_server_schedule_module_method() {
        let mut alloc = bump_allocator();
        let mod_class = empty_class("A");
        let proc_class = empty_process_class("A");
        let module = empty_module(&mut alloc, mod_class);
        let method = empty_method(&mut alloc, module.class(), *module, "foo");
        let mut server =
            OwnedServer::new(Server::alloc(&mut alloc, *proc_class));

        server.schedule_module_method(&mut alloc, module.class(), method);

        assert!(server.generator.is_some());

        Method::drop(method);
    }

    #[test]
    fn test_server_send_message() {
        let mut alloc = bump_allocator();
        let proc_class =
            OwnedClass::new(Class::process("A".to_string(), Vec::new(), 1));
        let mod_class = empty_class("A");
        let module = empty_module(&mut alloc, mod_class);
        let method = empty_method(&mut alloc, module.class(), *module, "foo");
        let index = unsafe { MethodIndex::new(0) };

        unsafe { proc_class.set_method(index, method) };

        let mut server =
            OwnedServer::new(Server::alloc(&mut alloc, *proc_class));

        server.send_message(index, &[Pointer::int(42)]);

        let mut state = server.state();

        assert_eq!(state.mailbox.messages.len(), 1);
        assert_eq!(state.mailbox.receive().unwrap().length, 1);
    }

    #[test]
    fn test_server_generator_to_run_without_a_generator() {
        let mut alloc = bump_allocator();
        let gen_class = empty_class("Generator");
        let class = empty_process_class("A");
        let mut server = OwnedServer::new(Server::alloc(&mut alloc, *class));

        assert!(server.generator_to_run(&mut alloc, *gen_class).is_none());
    }

    #[test]
    fn test_server_generator_to_run_waiting_server() {
        let mut alloc = bump_allocator();
        let gen_class = empty_class("Generator");
        let class = empty_process_class("A");
        let mut server = OwnedServer::new(Server::alloc(&mut alloc, *class));

        assert!(server.generator_to_run(&mut alloc, *gen_class).is_none());
        assert!(server.state().status.is_waiting_for_message());
        assert!(!server.state().status.is_waiting());
    }

    #[test]
    fn test_server_generator_to_run_with_message() {
        let mut alloc = bump_allocator();
        let proc_class =
            OwnedClass::new(Class::process("A".to_string(), Vec::new(), 1));
        let mod_class = empty_class("A");
        let gen_class = empty_class("Generator");
        let module = empty_module(&mut alloc, mod_class);
        let method = empty_method(&mut alloc, module.class(), *module, "foo");
        let index = unsafe { MethodIndex::new(0) };

        unsafe { proc_class.set_method(index, method) };

        let mut server =
            OwnedServer::new(Server::alloc(&mut alloc, *proc_class));

        server.send_message(index, &[Pointer::int(42)]);

        let gen = server.generator_to_run(&mut alloc, *gen_class).unwrap();

        assert!(server.generator.is_some());
        assert!(gen.context.get_register(0) == Pointer::int(42));
    }

    #[test]
    fn test_server_generator_to_run_with_existing_generator() {
        let mut alloc = bump_allocator();
        let proc_class =
            OwnedClass::new(Class::process("A".to_string(), Vec::new(), 1));
        let mod_class = empty_class("A");
        let gen_class = empty_class("Generator");
        let module = empty_module(&mut alloc, mod_class);
        let method = empty_method(&mut alloc, module.class(), *module, "foo");
        let ctx = ExecutionContext::for_method(method);
        let gen = Generator::alloc(&mut alloc, *gen_class, ctx);
        let mut server =
            OwnedServer::new(Server::alloc(&mut alloc, *proc_class));

        server.resume_generator(gen);

        assert!(
            server
                .generator_to_run(&mut alloc, *gen_class)
                .unwrap()
                .as_pointer()
                == gen.as_pointer()
        );

        Method::drop(method);
    }

    #[test]
    fn test_server_finish_generator_without_pending_work() {
        let mut alloc = bump_allocator();
        let class = empty_process_class("A");
        let server = OwnedServer::new(Server::alloc(&mut alloc, *class));

        assert_eq!(server.finish_generator(), Finished::Terminate);
    }

    #[test]
    fn test_server_finish_generator_with_clients() {
        let mut alloc = bump_allocator();
        let class = empty_process_class("A");
        let server = OwnedServer::new(Server::alloc(&mut alloc, *class));

        server.state().clients += 1;

        assert_eq!(server.finish_generator(), Finished::WaitForMessage);
        assert!(server.state().status.is_waiting_for_message());
    }

    #[test]
    fn test_server_finish_generator_with_messages() {
        let mut alloc = bump_allocator();
        let proc_class =
            OwnedClass::new(Class::process("A".to_string(), Vec::new(), 1));
        let mod_class = empty_class("A");
        let module = empty_module(&mut alloc, mod_class);
        let method = empty_method(&mut alloc, module.class(), *module, "foo");
        let index = unsafe { MethodIndex::new(0) };

        unsafe { proc_class.set_method(index, method) };

        let mut server =
            OwnedServer::new(Server::alloc(&mut alloc, *proc_class));

        server.send_message(index, &[]);

        assert_eq!(server.finish_generator(), Finished::Reschedule);
    }

    #[test]
    fn test_server_set_values() {
        let mut alloc = bump_allocator();
        let class = OwnedClass::new(Class::process(
            "A".to_string(),
            vec!["@a".to_string(), "@b".to_string()],
            0,
        ));
        let mut server = OwnedServer::new(Server::alloc(&mut alloc, *class));
        let idx0 = unsafe { FieldIndex::new(0) };
        let idx1 = unsafe { FieldIndex::new(1) };

        server.set_values(&[Pointer::int(4), Pointer::int(5)]);

        assert!(server.get(idx0) == Pointer::int(4));
        assert!(server.get(idx1) == Pointer::int(5));
    }

    #[test]
    fn test_server_get_set() {
        let mut alloc = bump_allocator();
        let class = OwnedClass::new(Class::process(
            "A".to_string(),
            vec!["@a".to_string()],
            0,
        ));
        let mut server = OwnedServer::new(Server::alloc(&mut alloc, *class));
        let idx = unsafe { FieldIndex::new(0) };

        server.set(idx, Pointer::int(4));

        assert!(server.get(idx) == Pointer::int(4));
    }

    #[test]
    fn test_server_timeout_expired() {
        let mut alloc = bump_allocator();
        let class = empty_process_class("A");
        let mut server = OwnedServer::new(Server::alloc(&mut alloc, *class));
        let timeout = Timeout::with_rc(Duration::from_secs(0));

        assert!(!server.timeout_expired());

        server.suspend(timeout);

        assert!(!server.timeout_expired());
        assert!(!server.state().status.timeout_expired());
    }

    #[test]
    fn test_server_pointer_identifier() {
        let ptr = unsafe { ServerPointer::new(Pointer::new(0x4 as *mut _)) };

        assert_eq!(ptr.identifier(), 0x4);
    }

    #[test]
    fn test_server_pointer_as_pointer() {
        let raw_ptr = unsafe { Pointer::new(0x4 as *mut _) };
        let ptr = unsafe { ServerPointer::new(raw_ptr) };

        assert_eq!(ptr.as_pointer(), raw_ptr);
    }

    #[test]
    fn test_client_new_and_drop() {
        let mut alloc = bump_allocator();
        let class = empty_process_class("A");
        let server = OwnedServer::new(Server::alloc(&mut alloc, *class));
        let client = Client::alloc(&mut alloc, *class, *server);

        assert_eq!(client.server().as_pointer(), server.as_pointer());
        assert_eq!(server.state().clients, 1);

        Client::drop(client);

        assert_eq!(server.state().clients, 0);
    }

    #[test]
    fn test_client_pointer_as_pointer() {
        let raw_ptr = unsafe { Pointer::new(0x4 as *mut _) };
        let ptr = unsafe { ClientPointer::new(raw_ptr) };

        assert_eq!(ptr.as_pointer(), raw_ptr);
    }

    #[test]
    fn test_future_new() {
        let mut alloc = bump_allocator();
        let fut_class = empty_class("Future");
        let state = FutureState::new();
        let future = Future::alloc(&mut alloc, *fut_class, state);

        unsafe {
            assert_eq!(
                future.get::<Header>().class.as_pointer(),
                fut_class.as_pointer()
            );
        }

        unsafe { Future::drop(future) };
    }

    #[test]
    fn test_future_reader_writer() {
        let mut alloc = bump_allocator();
        let fut_class = empty_class("Future");
        let (reader, writer) = Future::reader_writer(&mut alloc, *fut_class);

        unsafe {
            assert_eq!(
                reader.get::<Header>().class.as_pointer(),
                fut_class.as_pointer()
            );

            assert_eq!(
                writer.get::<Header>().class.as_pointer(),
                fut_class.as_pointer()
            );
        }

        unsafe { Future::drop(reader) };
        unsafe { Future::drop(writer) };
    }

    #[test]
    fn test_future_write_without_consumer() {
        let mut alloc = bump_allocator();
        let fut_class = empty_class("Future");
        let state = FutureState::new();
        let future = Future::alloc(&mut alloc, *fut_class, state);
        let fut_ref = unsafe { future.get_mut::<Future>() };
        let result = fut_ref.write(Pointer::int(42), false);

        assert_eq!(result, WriteResult::Continue);
        assert_eq!(fut_ref.state.lock().thrown, false);

        unsafe { Future::drop(future) };
    }

    #[test]
    fn test_future_write_thrown() {
        let mut alloc = bump_allocator();
        let fut_class = empty_class("Future");
        let state = FutureState::new();
        let future = Future::alloc(&mut alloc, *fut_class, state);
        let fut_ref = unsafe { future.get_mut::<Future>() };
        let result = fut_ref.write(Pointer::int(42), true);

        assert_eq!(result, WriteResult::Continue);
        assert_eq!(fut_ref.state.lock().thrown, true);

        unsafe { Future::drop(future) };
    }

    #[test]
    fn test_future_write_disconnected() {
        let mut alloc = bump_allocator();
        let fut_class = empty_class("Future");
        let state = FutureState::new();
        let future = Future::alloc(&mut alloc, *fut_class, state);
        let fut_ref = unsafe { future.get_mut::<Future>() };

        fut_ref.state.lock().disconnected = true;

        let result = fut_ref.write(Pointer::int(42), false);

        assert_eq!(result, WriteResult::Discard);

        unsafe { Future::drop(future) };
    }

    #[test]
    fn test_future_write_with_consumer() {
        let mut alloc = bump_allocator();
        let fut_class = empty_class("Future");
        let proc_class = empty_process_class("A");
        let server = OwnedServer::new(Server::alloc(&mut alloc, *proc_class));
        let state = FutureState::new();
        let future = Future::alloc(&mut alloc, *fut_class, state);
        let fut_ref = unsafe { future.get_mut::<Future>() };

        fut_ref.state.lock().consumer = Some(*server);

        let result = fut_ref.write(Pointer::int(42), false);

        assert_eq!(result, WriteResult::Continue);

        unsafe { Future::drop(future) };
    }

    #[test]
    fn test_future_write_with_waiting_consumer() {
        let mut alloc = bump_allocator();
        let fut_class = empty_class("Future");
        let proc_class = empty_process_class("A");
        let server = OwnedServer::new(Server::alloc(&mut alloc, *proc_class));
        let state = FutureState::new();
        let future = Future::alloc(&mut alloc, *fut_class, state);
        let fut_ref = unsafe { future.get_mut::<Future>() };

        server.state().waiting_for_future(None);
        fut_ref.state.lock().consumer = Some(*server);

        let result = fut_ref.write(Pointer::int(42), false);

        assert_eq!(result, WriteResult::Reschedule(*server));

        unsafe { Future::drop(future) };
    }

    #[test]
    fn test_future_write_with_waiting_consumer_with_timeout() {
        let mut alloc = bump_allocator();
        let fut_class = empty_class("Future");
        let proc_class = empty_process_class("A");
        let server = OwnedServer::new(Server::alloc(&mut alloc, *proc_class));
        let state = FutureState::new();
        let future = Future::alloc(&mut alloc, *fut_class, state);
        let fut_ref = unsafe { future.get_mut::<Future>() };
        let timeout = Timeout::with_rc(Duration::from_secs(0));

        server.state().waiting_for_future(Some(timeout));
        fut_ref.state.lock().consumer = Some(*server);

        let result = fut_ref.write(Pointer::int(42), false);

        assert_eq!(result, WriteResult::RescheduleWithTimeout(*server));
        assert_eq!(server.state().status.is_waiting_for_future(), false);
        assert!(server.state().timeout.is_none());

        unsafe { Future::drop(future) };
    }

    #[test]
    fn test_future_get_without_result() {
        let mut alloc = bump_allocator();
        let fut_class = empty_class("Future");
        let proc_class = empty_process_class("A");
        let server = OwnedServer::new(Server::alloc(&mut alloc, *proc_class));
        let state = FutureState::new();
        let future = Future::alloc(&mut alloc, *fut_class, state);
        let fut_ref = unsafe { future.get_mut::<Future>() };

        assert_eq!(fut_ref.get(*server, None), FutureResult::None);
        assert_eq!(fut_ref.state.lock().consumer, Some(*server));
        assert!(server.state().status.is_waiting_for_future());

        unsafe { Future::drop(future) };
    }

    #[test]
    fn test_future_get_without_result_with_timeout() {
        let mut alloc = bump_allocator();
        let fut_class = empty_class("Future");
        let proc_class = empty_process_class("A");
        let server = OwnedServer::new(Server::alloc(&mut alloc, *proc_class));
        let state = FutureState::new();
        let future = Future::alloc(&mut alloc, *fut_class, state);
        let fut_ref = unsafe { future.get_mut::<Future>() };
        let timeout = Timeout::with_rc(Duration::from_secs(0));

        assert_eq!(fut_ref.get(*server, Some(timeout)), FutureResult::None);
        assert_eq!(fut_ref.state.lock().consumer, Some(*server));
        assert!(server.state().status.is_waiting_for_future());
        assert!(server.state().timeout.is_some());

        unsafe { Future::drop(future) };
    }

    #[test]
    fn test_future_get_with_result() {
        let mut alloc = bump_allocator();
        let fut_class = empty_class("Future");
        let proc_class = empty_process_class("A");
        let server = OwnedServer::new(Server::alloc(&mut alloc, *proc_class));
        let state = FutureState::new();
        let future = Future::alloc(&mut alloc, *fut_class, state);
        let fut_ref = unsafe { future.get_mut::<Future>() };
        let value = Pointer::int(42);

        server.state().waiting_for_future(None);
        fut_ref.state.lock().result = Some(value);

        assert_eq!(fut_ref.get(*server, None), FutureResult::Returned(value));
        assert!(fut_ref.state.lock().consumer.is_none());
        assert!(!server.state().status.is_waiting_for_future());

        unsafe { Future::drop(future) };
    }

    #[test]
    fn test_future_get_with_thrown_result() {
        let mut alloc = bump_allocator();
        let fut_class = empty_class("Future");
        let proc_class = empty_process_class("A");
        let server = OwnedServer::new(Server::alloc(&mut alloc, *proc_class));
        let state = FutureState::new();
        let future = Future::alloc(&mut alloc, *fut_class, state);
        let fut_ref = unsafe { future.get_mut::<Future>() };
        let value = Pointer::int(42);

        server.state().waiting_for_future(None);
        fut_ref.state.lock().result = Some(value);
        fut_ref.state.lock().thrown = true;

        assert_eq!(fut_ref.get(*server, None), FutureResult::Thrown(value));
        assert!(fut_ref.state.lock().consumer.is_none());
        assert!(!server.state().status.is_waiting_for_future());

        unsafe { Future::drop(future) };
    }

    #[test]
    fn test_future_drop() {
        let mut alloc = bump_allocator();
        let fut_class = empty_class("Future");
        let (reader, writer) = Future::reader_writer(&mut alloc, *fut_class);

        unsafe {
            Future::drop(reader);
            assert!(writer.get::<Future>().state.lock().disconnected);
            Future::drop(writer);
        }
    }
}
