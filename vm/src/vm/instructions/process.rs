//! VM functions for working with Inko processes.
use crate::block::Block;
use crate::chunk::Chunk;
use crate::duration;
use crate::execution_context::ExecutionContext;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::{Process, RcProcess, RescheduleRights};
use crate::runtime_error::RuntimeError;
use crate::scheduler::process_worker::ProcessWorker;
use crate::scheduler::timeouts::Timeout;
use crate::vm::state::RcState;

// TODO: support arguments
#[inline(always)]
pub fn process_allocate(state: &RcState) -> RcProcess {
    let process = Process::new(state.global_allocator.clone(), &state.config);

    let self_object = process.allocate(
        object_value::process(process.clone()),
        state.process_prototype,
    );

    process.set_self_object(self_object);
    process
}

// TODO: remove in favour of just process_allocate()
#[inline(always)]
pub fn process_spawn(
    state: &RcState,
    current_process: &RcProcess,
    block_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let block = block_ptr.block_value()?;
    let new_proc = process_allocate(&state, &block);
    let new_ptr = current_process
        .allocate(object_value::process(new_proc), state.process_prototype);

    Ok(new_ptr)
}

#[inline(always)]
pub fn process_send_message(
    state: &RcState,
    context: &ExecutionContext,
    sender: &RcProcess,
    receiver: ObjectPointer,
    msg: ObjectPointer,
    start_reg: u16,
    amount: u16,
) -> Result<ObjectPointer, RuntimeError> {
    let rec = receiver.process_value()?;
    let mut args = Chunk::new(amount as usize);

    for (index, register) in
        (start_reg..(start_reg + amount)).into_iter().enumerate()
    {
        args[index] = context.get_register(register);
    }

    let future = if rec == sender {
        rec.send_message_from_self(sender.clone(), msg, args)
    } else {
        let (future, resched) =
            rec.send_message_from_external_process(sender.clone(), msg, args)?;

        reschedule_process(state, rec, resched);
        future
    };

    let future_ptr =
        sender.allocate(object_value::future(future), state.future_prototype);

    Ok(future_ptr)
}

#[inline(always)]
pub fn process_suspend_current(
    state: &RcState,
    process: &RcProcess,
    timeout_ptr: ObjectPointer,
) -> Result<(), String> {
    if let Some(duration) = duration::from_f64(timeout_ptr.float_value()?) {
        state.timeout_worker.park(process.clone(), duration);
    } else {
        state.scheduler.schedule(process.clone());
    }

    Ok(())
}

#[inline(always)]
pub fn process_set_blocking(
    state: &RcState,
    process: &RcProcess,
    blocking_ptr: ObjectPointer,
) -> ObjectPointer {
    let is_blocking = blocking_ptr == state.true_object;

    if process.is_pinned() || is_blocking == process.is_blocking() {
        // If an process is pinned we can't move it to another pool. We can't
        // panic in this case, since it would prevent code from using certain IO
        // operations that may try to move the process to another pool.
        //
        // Instead, we simply ignore the request and continue running on the
        // current thread.
        state.false_object
    } else {
        process.set_blocking(is_blocking);
        state.true_object
    }
}

#[inline(always)]
pub fn process_add_defer_to_caller(
    process: &RcProcess,
    block: ObjectPointer,
) -> Result<ObjectPointer, String> {
    if block.block_value().is_err() {
        return Err("only Blocks can be deferred".to_string());
    }

    let context = process.context_mut();

    // We can not use `if let Some(...) = ...` here as the mutable borrow of
    // "context" prevents the 2nd mutable borrow inside the "else".
    if context.parent().is_some() {
        context.parent_mut().unwrap().add_defer(block);
    } else {
        context.add_defer(block);
    }

    Ok(block)
}

#[inline(always)]
pub fn process_set_pinned(
    state: &RcState,
    process: &RcProcess,
    worker: &mut ProcessWorker,
    pinned: ObjectPointer,
) -> ObjectPointer {
    if pinned == state.true_object {
        let result = if process.thread_id().is_some() {
            state.false_object
        } else {
            process.set_thread_id(worker.id as u8);
            state.true_object
        };

        worker.enter_exclusive_mode();
        result
    } else {
        process.unset_thread_id();
        worker.leave_exclusive_mode();
        state.false_object
    }
}

#[inline(always)]
pub fn process_identifier(
    state: &RcState,
    current_process: &RcProcess,
    process_ptr: ObjectPointer,
) -> Result<ObjectPointer, String> {
    let proc = process_ptr.process_value()?;
    let proto = state.string_prototype;
    let identifier = current_process.allocate_usize(proc.identifier(), proto);

    Ok(identifier)
}

#[inline(always)]
pub fn process_unwind_until_defining_scope(process: &RcProcess) {
    let top_binding = process.context().top_binding_pointer();

    loop {
        let context = process.context();

        if context.binding_pointer() == top_binding || process.pop_context() {
            return;
        }
    }
}

#[inline(always)]
pub fn future_get(
    state: &RcState,
    process: &RcProcess,
    future_ptr: ObjectPointer,
    timeout_ptr: ObjectPointer,
) -> Result<Option<ObjectPointer>, RuntimeError> {
    let mut shared = process.shared_data();
    let consumer = future_ptr.future_value()?;
    let mut future = consumer.lock();

    future.no_longer_waiting();

    if shared.timeout_expired() {
        return Ok(Some(ObjectPointer::null()));
    }

    if let Some(result) = future.result() {
        return Ok(Some(result));
    }

    let timeout = duration::from_f64(timeout_ptr.float_value()?)
        .map(|dur| Timeout::with_rc(dur));

    future.waiting();
    shared.park_for_future(timeout.clone());

    // Unlock explicitly so we don't keep the locks around too long while
    // suspending the process.
    drop(future);
    drop(shared);

    if let Some(timeout) = timeout {
        state.timeout_worker.suspend(process.clone(), timeout);
    }

    Ok(None)
}

#[inline(always)]
pub fn future_set_result(
    state: &RcState,
    process: &RcProcess,
    result: ObjectPointer,
    thrown: bool,
) -> Result<bool, String> {
    let future = if let Some(fut) = process.take_future() {
        fut
    } else {
        return Ok(false);
    };

    if let Some((rights, waiter)) =
        future.set_result(process, result, thrown)?
    {
        reschedule_process(state, waiter, rights);
    }

    Ok(true)
}

#[inline(always)]
pub fn future_ready(
    state: &RcState,
    future_ptr: ObjectPointer,
) -> Result<ObjectPointer, RuntimeError> {
    let future = future_ptr.future_value()?;
    let result = if future.lock().result().is_some() {
        state.false_object
    } else {
        state.true_object
    };

    Ok(result)
}

#[inline(always)]
pub fn future_wait(
    state: &RcState,
    process: &RcProcess,
    wait_ptr: ObjectPointer,
    timeout_ptr: ObjectPointer,
) -> Result<Option<ObjectPointer>, RuntimeError> {
    let mut shared = process.shared_data();
    let mut ready = 0;

    if shared.timeout_expired() {
        return Ok(Some(ObjectPointer::null()));
    }

    for fut_ptr in wait_ptr.array_value()? {
        let consumer = fut_ptr.future_value()?;
        let mut future = consumer.lock();

        // We need to unset the waiting flag so other processes writing to our
        // futures don't reschedule our process at the wrong time.
        future.no_longer_waiting();

        if future.result().is_some() {
            ready += 1;
        }
    }

    if ready > 0 {
        return Ok(Some(
            process.allocate_usize(ready, state.integer_prototype),
        ));
    }

    for fut_ptr in wait_ptr.array_value()? {
        fut_ptr.future_value()?.lock().waiting();
    }

    let timeout = duration::from_f64(timeout_ptr.float_value()?)
        .map(|dur| Timeout::with_rc(dur));

    shared.park_for_future(timeout.clone());

    // Unlock before rescheduling, since we don't need to keep the lock for
    // that.
    drop(shared);

    if let Some(timeout) = timeout {
        state.timeout_worker.suspend(process.clone(), timeout);
    }

    Ok(None)
}

#[inline(always)]
pub fn process_finish_message(
    state: &RcState,
    worker: &mut ProcessWorker,
    process: &RcProcess,
) -> Result<(), String> {
    if process.schedule_next_message()? {
        state.scheduler.schedule(process.clone());

        return Ok(());
    }

    if process.is_pinned() {
        // A pinned process can only run on the corresponding worker. Because
        // pinned workers won't run already unpinned processes, and because
        // processes can't be pinned until they run, this means there will only
        // ever be one process that triggers this code.
        worker.leave_exclusive_mode();
    }

    // Terminate once the main process has finished execution.
    if process.is_main() {
        state.terminate(0);
    }
    // For all other processes, we terminate when there are no more references
    // to our process, and there are no more messages to process.
    else if process.references() == 1 {
        // It's possible a message may have been sent just after we last
        // checked for a message, so we check once more.
        if process.schedule_next_message()? {
            state.scheduler.schedule(process.clone());
        } else {
            process.terminate();
        }
    } else {
        process.shared_data().park_for_message();
    }

    Ok(())
}

fn reschedule_process<T: Into<RcProcess>>(
    state: &RcState,
    process: T,
    rights: RescheduleRights,
) {
    match rights {
        RescheduleRights::AcquiredWithTimeout => {
            state.timeout_worker.increase_expired_timeouts();
            state.scheduler.schedule(process.into());
        }
        RescheduleRights::Acquired => {
            state.scheduler.schedule(process.into());
        }
        _ => {}
    }
}
