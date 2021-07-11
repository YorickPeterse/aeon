//! Functions for Inko futures.
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::process::{Future, ServerPointer};
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;

/// Drops a future.
///
/// This function requires one argument: the future to drop.
pub fn future_drop(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe {
        Future::drop(arguments[0]);
    }

    Ok(state.permanent_space.nil_singleton)
}

register!(future_drop);
