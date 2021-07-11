//! VM functions for working with Inko futures.
use crate::duration;
use crate::mem::allocator::Pointer;
use crate::mem::objects::Float;
use crate::mem::process::{Future, FutureResult, ServerPointer, WriteResult};
use crate::runtime_error::RuntimeError;
use crate::scheduler::process_worker::ProcessWorker;
use crate::scheduler::timeouts::Timeout;
use crate::vm::state::State;

#[inline(always)]
pub fn allocate(
    state: &State,
    worker: &mut ProcessWorker,
) -> (Pointer, Pointer) {
    Future::reader_writer(
        worker.allocator(),
        state.permanent_space.future_class(),
    )
}

#[inline(always)]
pub fn get(
    _: &State,
    process: ServerPointer,
    future_ptr: Pointer,
) -> Result<Option<Pointer>, RuntimeError> {
    let fut = unsafe { future_ptr.get::<Future>() };

    match fut.get(process, None) {
        FutureResult::Returned(val) => Ok(Some(val)),
        FutureResult::Thrown(val) => Err(RuntimeError::Error(val)),
        FutureResult::None => Ok(None),
    }
}

#[inline(always)]
pub fn get_with_timeout(
    state: &State,
    process: ServerPointer,
    future_ptr: Pointer,
    time_ptr: Pointer,
) -> Result<Option<Pointer>, RuntimeError> {
    if process.timeout_expired() {
        state.timeout_worker.increase_expired_timeouts();

        // When a previous call to this method didn't produce a result, it will
        // be retried either when a value is present or the timeout expired.
        return Ok(Some(state.permanent_space.undefined_singleton));
    }

    let timeout =
        Timeout::with_rc(duration::from_f64(unsafe { Float::read(time_ptr) }));
    let fut_ref = unsafe { future_ptr.get::<Future>() };

    match fut_ref.get(process, Some(timeout.clone())) {
        FutureResult::Returned(val) => Ok(Some(val)),
        FutureResult::Thrown(val) => Err(RuntimeError::Error(val)),
        FutureResult::None => {
            state.timeout_worker.suspend(process, timeout);
            Ok(None)
        }
    }
}

#[inline(always)]
pub fn write(
    state: &State,
    _: ServerPointer,
    future: Pointer,
    result: Pointer,
    thrown: bool,
) -> Result<Pointer, RuntimeError> {
    let fut_ref = unsafe { future.get_mut::<Future>() };

    match fut_ref.write(result, thrown) {
        WriteResult::Continue => {}
        WriteResult::Reschedule(consumer) => {
            state.scheduler.schedule(consumer);
        }
        WriteResult::RescheduleWithTimeout(consumer) => {
            state.timeout_worker.increase_expired_timeouts();
            state.scheduler.schedule(consumer);
        }
        WriteResult::Discard => {
            return Err(RuntimeError::Error(result));
        }
    }

    Ok(state.permanent_space.nil_singleton)
}
