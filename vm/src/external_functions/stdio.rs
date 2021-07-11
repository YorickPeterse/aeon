//! Functions for working with the standard input/output streams.
use crate::external_functions::read_into;
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{ByteArray, String as InkoString, UnsignedInt};
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;
use std::io::Write;
use std::io::{stderr, stdin, stdout};

/// Writes a String to STDOUT.
///
/// This function requires a single argument: the input to write.
pub fn stdout_write_string(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let input = unsafe { InkoString::read(&arguments[0]).as_bytes() };
    let size = stdout().write(&input)? as u64;

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        size,
    ))
}

/// Writes a ByteArray to STDOUT.
///
/// This function requires a single argument: the input to write.
pub fn stdout_write_bytes(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let input = unsafe { arguments[0].get::<ByteArray>() }.value();
    let size = stdout().write(&input)? as u64;

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        size,
    ))
}

/// Writes a String to STDERR.
///
/// This function requires a single argument: the input to write.
pub fn stderr_write_string(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let input = unsafe { InkoString::read(&arguments[0]).as_bytes() };
    let size = stderr().write(&input)? as u64;

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        size,
    ))
}

/// Writes a ByteArray to STDERR.
///
/// This function requires a single argument: the input to write.
pub fn stderr_write_bytes(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let input = unsafe { arguments[0].get::<ByteArray>() }.value();
    let size = stderr().write(&input)? as u64;

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        size,
    ))
}

/// Flushes to STDOUT.
///
/// This function doesn't require any arguments.
pub fn stdout_flush(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    stdout().flush()?;
    Ok(state.permanent_space.nil_singleton)
}

/// Flushes to STDERR.
///
/// This function doesn't require any arguments.
pub fn stderr_flush(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    stderr().flush()?;
    Ok(state.permanent_space.nil_singleton)
}

/// Reads bytes from STDIN.
///
/// This function requires the following arguments:
///
/// 1. The ByteArray to read the data into.
/// 2. The number of bytes to read.
pub fn stdin_read(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let buff = unsafe { arguments[0].get_mut::<ByteArray>() }.value_mut();
    let size = unsafe { UnsignedInt::read(arguments[1]) };
    let result = read_into(&mut stdin(), buff, size)?;

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        result,
    ))
}

register!(
    stdout_write_string,
    stdout_write_bytes,
    stderr_write_string,
    stderr_write_bytes,
    stdout_flush,
    stderr_flush,
    stdin_read
);
