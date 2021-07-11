//! Functions for working with OS commands.
use crate::external_functions::read_into;
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{
    Array, ByteArray, String as InkoString, UnsignedInt,
};
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;
use std::io::Write;
use std::process::{Child, Command, Stdio};

/// Spawns a child process.
///
/// This function requires the following arguments:
///
/// 1. The program name/path to run
/// 2. The arguments to pass to the command
/// 3. The environment variables to pass, as an array of key/value pairs (each
///    key and value are a separate value in the array)
/// 4. What to do with the STDIN stream
/// 5. What to do with the STDOUT stream
/// 6. What to do with the STDERR stream
/// 7. The working directory to use for the command. If the path is empty, no
///    custom directory is set
pub fn child_process_spawn(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let program = unsafe { InkoString::read(&arguments[0]) };
    let args = unsafe { arguments[1].get::<Array>() }.value();
    let env = unsafe { arguments[2].get::<Array>() }.value();
    let stdin = unsafe { UnsignedInt::read(arguments[3]) };
    let stdout = unsafe { UnsignedInt::read(arguments[4]) };
    let stderr = unsafe { UnsignedInt::read(arguments[5]) };
    let directory = unsafe { InkoString::read(&arguments[6]) };
    let mut cmd = Command::new(program);

    for ptr in args {
        cmd.arg(unsafe { InkoString::read(&*ptr) });
    }

    for pair in env.chunks(2) {
        unsafe {
            cmd.env(InkoString::read(&&pair[0]), InkoString::read(&&pair[1]));
        }
    }

    cmd.stdin(stdio_for(stdin));
    cmd.stdout(stdio_for(stdout));
    cmd.stderr(stdio_for(stderr));

    if !directory.is_empty() {
        cmd.current_dir(directory);
    }

    Ok(Pointer::boxed(cmd.spawn()?))
}

/// Waits for a command and returns its exit status.
///
/// This method blocks the current thread while waiting.
///
/// This function requires a single argument: the command to wait for.
///
/// This function closes STDIN before waiting.
pub fn child_process_wait(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };
    let status = child.wait()?;
    let code = status.code().unwrap_or(0) as i64;

    Ok(Pointer::int(code))
}

/// Waits for a command and returns its exit status, without blocking.
///
/// This method returns immediately if the child has not yet been terminated. If
/// the process hasn't terminated yet, -1 is returned.
///
/// This function requires a single argument: the command to wait for.
pub fn child_process_try_wait(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };
    let status = child.try_wait()?;
    let code = status.map(|s| s.code().unwrap_or(0)).unwrap_or(-1) as i64;

    Ok(Pointer::int(code))
}

/// Reads from a child process' STDOUT stream.
///
/// This function requires the following arguments:
///
/// 1. The command to read from
/// 2. The ByteArray to read the data into
/// 3. The number of bytes to read
pub fn child_process_stdout_read(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };
    let buff = unsafe { arguments[1].get_mut::<ByteArray>() }.value_mut();
    let size = unsafe { UnsignedInt::read(arguments[2]) };
    let value = child
        .stdout
        .as_mut()
        .map(|stream| read_into(stream, buff, size))
        .unwrap_or(Ok(0))?;

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        value,
    ))
}

/// Reads from a child process' STDERR stream.
///
/// This function requires the following arguments:
///
/// 1. The command to read from
/// 2. The ByteArray to read the data into
/// 3. The number of bytes to read
pub fn child_process_stderr_read(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };
    let buff = unsafe { arguments[1].get_mut::<ByteArray>() }.value_mut();
    let size = unsafe { UnsignedInt::read(arguments[2]) };
    let value = child
        .stderr
        .as_mut()
        .map(|stream| read_into(stream, buff, size))
        .unwrap_or(Ok(0))?;

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        value,
    ))
}

/// Writes a ByteArray to a child process' STDIN stream.
///
/// This function requires the following arguments:
///
/// 1. The command to write to.
/// 2. The ByteArray to write.
pub fn child_process_stdin_write_bytes(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };
    let input = unsafe { arguments[1].get::<ByteArray>() }.value();
    let value = child
        .stdin
        .as_mut()
        .map(|stream| stream.write(&input))
        .unwrap_or(Ok(0))?;

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        value as u64,
    ))
}

/// Writes a String to a child process' STDIN stream.
///
/// This function requires the following arguments:
///
/// 1. The command to write to.
/// 2. The String to write.
pub fn child_process_stdin_write_string(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };
    let input = unsafe { InkoString::read(&arguments[1]) };
    let value = child
        .stdin
        .as_mut()
        .map(|stream| stream.write(input.as_bytes()))
        .unwrap_or(Ok(0))?;

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        value as u64,
    ))
}

/// Flushes the child process' STDIN stream.
///
/// This function requires the following arguments:
///
/// 1. The command to flush STDIN for.
pub fn child_process_stdin_flush(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };

    child
        .stdin
        .as_mut()
        .map(|stream| stream.flush())
        .unwrap_or(Ok(()))?;

    Ok(state.permanent_space.nil_singleton)
}

/// Closes the STDOUT stream of a child process.
///
/// This function requires a single argument: the command to close the stream
/// for.
pub fn child_process_stdout_close(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };

    child.stdout.take();
    Ok(state.permanent_space.nil_singleton)
}

/// Closes the STDERR stream of a child process.
///
/// This function requires a single argument: the command to close the stream
/// for.
pub fn child_process_stderr_close(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };

    child.stderr.take();
    Ok(state.permanent_space.nil_singleton)
}

/// Closes the STDIN stream of a child process.
///
/// This function requires a single argument: the command to close the stream
/// for.
pub fn child_process_stdin_close(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let child = unsafe { arguments[0].get_mut::<Child>() };

    child.stdin.take();
    Ok(state.permanent_space.nil_singleton)
}

/// Drops a child process.
///
/// This function requires a single argument: the child process to drop.
pub fn child_process_drop(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe {
        arguments[0].drop_boxed::<Child>();
    }

    Ok(state.permanent_space.nil_singleton)
}

fn stdio_for(value: u64) -> Stdio {
    match value {
        1 => Stdio::inherit(),
        2 => Stdio::piped(),
        _ => Stdio::null(),
    }
}

register!(
    child_process_spawn,
    child_process_wait,
    child_process_try_wait,
    child_process_stdout_read,
    child_process_stderr_read,
    child_process_stdout_close,
    child_process_stderr_close,
    child_process_stdin_close,
    child_process_stdin_write_bytes,
    child_process_stdin_write_string,
    child_process_stdin_flush,
    child_process_drop
);
