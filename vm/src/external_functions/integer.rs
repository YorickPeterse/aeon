//! Functions for working with Inko integers.
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{Float, Int, String as InkoString};
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;

/// Converts an Integer to a Float.
///
/// This function requires a single argument: the integer to convert.
pub fn int_to_float(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let value = unsafe { Int::read(arguments[0]) } as f64;

    Ok(Float::alloc(
        alloc,
        state.permanent_space.float_class(),
        value,
    ))
}

/// Converts an Integer to a String.
///
/// This function requires a single argument: the integer to convert.
pub fn int_to_string(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let value = unsafe { Int::read(arguments[0]) }.to_string();

    Ok(InkoString::alloc(
        alloc,
        state.permanent_space.string_class(),
        value,
    ))
}

/// Copies an Integer
///
/// This function requires a single argument: the Integer to copy.
pub fn int_clone(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let value = unsafe { Int::read(arguments[0]) };

    Ok(Int::alloc(alloc, state.permanent_space.int_class(), value))
}

register!(int_to_float, int_to_string, int_clone);
