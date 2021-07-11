//! Functions for working with Inko arrays.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::immutable_string::ImmutableString;
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{ByteArray, String as InkoString};
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;

/// Removes all values from a ByteArray.
///
/// This function requires a single argument: the ByteArray to clear.
pub fn byte_array_clear(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let bytes = unsafe { arguments[0].get_mut::<ByteArray>() };

    bytes.value_mut().clear();
    Ok(state.permanent_space.nil_singleton)
}

/// Converts a ByteArray to a String.
///
/// This function requires a single argument: the ByteArray to convert.
pub fn byte_array_to_string(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let bytes = unsafe { arguments[0].get::<ByteArray>() }.value();
    let string = ImmutableString::from_utf8(bytes.clone());

    Ok(InkoString::from_immutable_string(
        alloc,
        state.permanent_space.string_class(),
        ArcWithoutWeak::new(string),
    ))
}

/// Converts a ByteArray to a String by taking ownership of the byte array.
///
/// This function requires a single argument: the ByteArray to convert.
pub fn byte_array_into_string(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let bytes = unsafe { arguments[0].get_mut::<ByteArray>() }.take_bytes();
    let string = ImmutableString::from_utf8(bytes);

    Ok(InkoString::from_immutable_string(
        alloc,
        state.permanent_space.string_class(),
        ArcWithoutWeak::new(string),
    ))
}

/// Drops the given ByteArray.
///
/// This function requires a single argument: the ByteArray to drop.
pub fn byte_array_drop(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe {
        ByteArray::drop(arguments[0]);
    }

    Ok(state.permanent_space.nil_singleton)
}

register!(
    byte_array_clear,
    byte_array_to_string,
    byte_array_into_string,
    byte_array_drop
);
