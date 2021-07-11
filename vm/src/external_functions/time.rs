//! Functions for system and monotonic clocks.
use crate::date_time::DateTime;
use crate::duration;
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{Float, Int};
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;

/// Returns the current time using a monotonically increasing clock.
pub fn time_monotonic(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let duration = state.start_time.elapsed();
    let seconds = duration::to_f64(duration);

    Ok(Float::alloc(
        alloc,
        state.permanent_space.float_class(),
        seconds,
    ))
}

/// Returns the current system time.
pub fn time_system(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let time = DateTime::now().timestamp();

    Ok(Float::alloc(
        alloc,
        state.permanent_space.float_class(),
        time,
    ))
}

/// Returns the current system time offset relative to UTC.
pub fn time_offset(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let offset = DateTime::now().utc_offset();

    Ok(Int::alloc(alloc, state.permanent_space.int_class(), offset))
}

register!(time_monotonic, time_system, time_offset);
