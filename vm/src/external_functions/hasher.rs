//! Functions for hashing objects.
use crate::hasher::Hasher;
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{Float, Int, String as InkoString, UnsignedInt};
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;

/// Creates a new hasher.
///
/// This function requires the following arguments:
///
/// 1. The first key to use to seed the hasher.
/// 2. The second key to use to seed the hasher.
pub fn hasher_new(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let key0 = unsafe { UnsignedInt::read(arguments[0]) };
    let key1 = unsafe { UnsignedInt::read(arguments[1]) };

    Ok(Pointer::boxed(Hasher::new(key0, key1)))
}

/// Writes an arbitrary object to a hasher.
///
/// This function requires the following arguments:
///
/// 1. The hasher to write to.
/// 2. The object to write to the hasher.
pub fn hasher_write_object(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let hasher = unsafe { arguments[0].get_mut::<Hasher>() };
    let value = arguments[1];

    hasher.write_unsigned_int(value.untagged_ptr() as u64);
    Ok(state.permanent_space.nil_singleton)
}

/// Writes a signed integer to a hasher.
///
/// This function requires the following arguments:
///
/// 1. The hasher to write to.
/// 2. The value to write to the hasher.
pub fn hasher_write_int(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let hasher = unsafe { arguments[0].get_mut::<Hasher>() };
    let value = unsafe { Int::read(arguments[1]) };

    hasher.write_int(value);
    Ok(state.permanent_space.nil_singleton)
}

/// Writes an unsigned integer to a hasher.
///
/// This function requires the following arguments:
///
/// 1. The hasher to write to.
/// 2. The value to write to the hasher.
pub fn hasher_write_unsigned_int(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let hasher = unsafe { arguments[0].get_mut::<Hasher>() };
    let value = unsafe { UnsignedInt::read(arguments[1]) };

    hasher.write_unsigned_int(value);
    Ok(state.permanent_space.nil_singleton)
}

/// Writes a float to a hasher.
///
/// This function requires the following arguments:
///
/// 1. The hasher to write to.
/// 2. The value to write to the hasher.
pub fn hasher_write_float(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let hasher = unsafe { arguments[0].get_mut::<Hasher>() };
    let value = unsafe { Float::read(arguments[1]) };

    hasher.write_float(value);
    Ok(state.permanent_space.nil_singleton)
}

/// Writes a string to a hasher.
///
/// This function requires the following arguments:
///
/// 1. The hasher to write to.
/// 2. The value to write to the hasher.
pub fn hasher_write_string(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let hasher = unsafe { arguments[0].get_mut::<Hasher>() };
    let value = unsafe { InkoString::read(&arguments[1]) };

    hasher.write_string(value);
    Ok(state.permanent_space.nil_singleton)
}

/// Gets a hash from a hasher.
///
/// This function requires a single argument: the hasher to get the hash from.
pub fn hasher_to_hash(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let hasher = unsafe { arguments[0].get_mut::<Hasher>() };
    let value = hasher.finish();

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        value,
    ))
}

/// Drops a hasher.
///
/// This function requires a single argument: the hasher to drop.
pub fn hasher_drop(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe {
        arguments[0].drop_boxed::<Hasher>();
    }

    Ok(state.permanent_space.nil_singleton)
}

register!(
    hasher_new,
    hasher_write_object,
    hasher_write_int,
    hasher_write_unsigned_int,
    hasher_write_float,
    hasher_write_string,
    hasher_to_hash,
    hasher_drop
);
