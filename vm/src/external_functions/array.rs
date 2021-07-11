//! Functions for working with Inko arrays.
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::Array;
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;

/// Removes all values from an array.
///
/// This function requires a single argument: the array to clear.
pub fn array_clear(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let array = unsafe { arguments[0].get_mut::<Array>() }.value_mut();

    array.clear();
    Ok(state.permanent_space.nil_singleton)
}

/// Drops the given array.
///
/// This function requires a single argument: the array to drop.
pub fn array_drop(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe {
        Array::drop(arguments[0]);
    }

    Ok(state.permanent_space.nil_singleton)
}

register!(array_clear, array_drop);
