//! Functions for working with the file system.
//!
//! Files aren't allocated onto the Inko heap. Instead, we allocate them using
//! Rust's allocator and convert them into an Inko pointer. Dropping a file
//! involves turning that pointer back into a File, then dropping the Rust
//! object.
//!
//! This approach means the VM doesn't need to know anything about what objects
//! to use for certain files, how to store file paths, etc; instead we can keep
//! all that in the standard library.
use crate::date_time::DateTime;
use crate::external_functions::read_into;
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{
    Array, ByteArray, Float, Int, String as InkoString, UnsignedInt,
};
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};

/// Drops a File.
///
/// This function requires a single argument: the file to drop
pub fn file_drop(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe {
        arguments[0].drop_boxed::<File>();
    }

    Ok(state.permanent_space.nil_singleton)
}

/// Seeks a file to an offset.
///
/// This function takes the following arguments:
///
/// 1. The file to seek for.
/// 2. The byte offset to seek to.
pub fn file_seek(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let file = unsafe { arguments[0].get_mut::<File>() };
    let offset = unsafe { Int::read(arguments[1]) };
    let seek = if offset < 0 {
        SeekFrom::End(offset)
    } else {
        SeekFrom::Start(offset as u64)
    };

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.int_class(),
        file.seek(seek)?,
    ))
}

/// Flushes a file.
///
/// This function requires a single argument: the file to flush.
pub fn file_flush(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let file = unsafe { arguments[0].get_mut::<File>() };

    file.flush()?;
    Ok(state.permanent_space.nil_singleton)
}

/// Writes a String to a file.
///
/// This function requires the following arguments:
///
/// 1. The file to write to.
/// 2. The input to write.
pub fn file_write_string(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let file = unsafe { arguments[0].get_mut::<File>() };
    let input = unsafe { InkoString::read(&arguments[1]).as_bytes() };

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.int_class(),
        file.write(&input)? as u64,
    ))
}

/// Writes a ByteArray to a file.
///
/// This function requires the following arguments:
///
/// 1. The file to write to.
/// 2. The input to write.
pub fn file_write_bytes(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let file = unsafe { arguments[0].get_mut::<File>() };
    let input = unsafe { arguments[1].get::<ByteArray>() };

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.int_class(),
        file.write(input.value())? as u64,
    ))
}

/// Copies a file from one location to another.
///
/// This function requires the following arguments:
///
/// 1. The path to the file to copy.
/// 2. The path to copy the file to.
pub fn file_copy(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let src = unsafe { InkoString::read(&arguments[0]) };
    let dst = unsafe { InkoString::read(&arguments[1]) };

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.int_class(),
        fs::copy(src, dst)?,
    ))
}

/// Returns the size of a file in bytes.
///
/// This function requires a single argument: the path of the file to return the
/// size for.
pub fn file_size(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.int_class(),
        fs::metadata(path)?.len(),
    ))
}

/// Removes a file.
///
/// This function requires a single argument: the path to the file to remove.
pub fn file_remove(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    fs::remove_file(path)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Returns the creation time of a path.
///
/// This function requires one argument: the path to obtain the time for.
pub fn path_created_at(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };
    let time =
        DateTime::from_system_time(fs::metadata(path)?.created()?).timestamp();

    Ok(Float::alloc(
        alloc,
        state.permanent_space.float_class(),
        time,
    ))
}

/// Returns the modification time of a path.
///
/// This function requires one argument: the path to obtain the time for.
pub fn path_modified_at(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };
    let time =
        DateTime::from_system_time(fs::metadata(path)?.modified()?).timestamp();

    Ok(Float::alloc(
        alloc,
        state.permanent_space.float_class(),
        time,
    ))
}

/// Returns the access time of a path.
///
/// This function requires one argument: the path to obtain the time for.
pub fn path_accessed_at(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };
    let time =
        DateTime::from_system_time(fs::metadata(path)?.accessed()?).timestamp();

    Ok(Float::alloc(
        alloc,
        state.permanent_space.float_class(),
        time,
    ))
}

/// Checks if a path is a file.
///
/// This function requires a single argument: the path to check.
pub fn path_is_file(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    if fs::metadata(path).map(|m| m.is_file()).unwrap_or(false) {
        Ok(state.permanent_space.true_singleton)
    } else {
        Ok(state.permanent_space.false_singleton)
    }
}

/// Checks if a path is a directory.
///
/// This function requires a single argument: the path to check.
pub fn path_is_directory(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    if fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false) {
        Ok(state.permanent_space.true_singleton)
    } else {
        Ok(state.permanent_space.false_singleton)
    }
}

/// Checks if a path exists.
///
/// This function requires a single argument: the path to check.
pub fn path_exists(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    if fs::metadata(path).is_ok() {
        Ok(state.permanent_space.true_singleton)
    } else {
        Ok(state.permanent_space.false_singleton)
    }
}

/// Opens a file in read-only mode.
///
/// This function requires one argument: the path to the file to open.
pub fn file_open_read_only(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let mut opts = OpenOptions::new();

    opts.read(true);
    Ok(open_file(opts, arguments[0]))
}

/// Opens a file in write-only mode.
///
/// This function requires one argument: the path to the file to open.
pub fn file_open_write_only(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let mut opts = OpenOptions::new();

    opts.write(true).truncate(true).create(true);
    Ok(open_file(opts, arguments[0]))
}

/// Opens a file in append-only mode.
///
/// This function requires one argument: the path to the file to open.
pub fn file_open_append_only(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let mut opts = OpenOptions::new();

    opts.append(true).create(true);
    Ok(open_file(opts, arguments[0]))
}

/// Opens a file for both reading and writing.
///
/// This function requires one argument: the path to the file to open.
pub fn file_open_read_write(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let mut opts = OpenOptions::new();

    opts.read(true).write(true).create(true);
    Ok(open_file(opts, arguments[0]))
}

/// Opens a file for both reading and appending.
///
/// This function requires one argument: the path to the file to open.
pub fn file_open_read_append(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let mut opts = OpenOptions::new();

    opts.read(true).append(true).create(true);
    Ok(open_file(opts, arguments[0]))
}

/// Reads bytes from a file into a ByteArray.
///
/// This function requires the following arguments:
///
/// 1. The file to read from.
/// 2. A ByteArray to read into.
/// 3. The number of bytes to read.
pub fn file_read(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let file = unsafe { arguments[0].get_mut::<File>() };
    let buff = unsafe { arguments[1].get_mut::<ByteArray>() };
    let size = unsafe { UnsignedInt::read(arguments[2]) };

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.int_class(),
        read_into(file, buff.value_mut(), size)?,
    ))
}

/// Creates a new directory.
///
/// This function requires one argument: the path of the directory to create.
pub fn directory_create(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    fs::create_dir(path)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Creates a new directory and any missing parent directories.
///
/// This function requires one argument: the path of the directory to create.
pub fn directory_create_recursive(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    fs::create_dir_all(path)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Removes a directory.
///
/// This function requires one argument: the path of the directory to remove.
pub fn directory_remove(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    fs::remove_dir(path)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Removes a directory and all its contents.
///
/// This function requires one argument: the path of the directory to remove.
pub fn directory_remove_recursive(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };

    fs::remove_dir_all(path)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Returns the contents of a directory.
///
/// This function requires one argument: the path of the directory to list.
pub fn directory_list(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = unsafe { InkoString::read(&arguments[0]) };
    let mut paths = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path().to_string_lossy().to_string();
        let pointer = InkoString::alloc(
            alloc,
            state.permanent_space.string_class(),
            path,
        );

        paths.push(pointer);
    }

    Ok(Array::alloc(
        alloc,
        state.permanent_space.array_class(),
        paths,
    ))
}

fn open_file(options: OpenOptions, path_ptr: Pointer) -> Pointer {
    let path = unsafe { InkoString::read(&path_ptr) };

    Pointer::boxed(options.open(path))
}

register!(
    file_drop,
    file_seek,
    file_flush,
    file_write_string,
    file_write_bytes,
    file_copy,
    file_size,
    file_remove,
    path_created_at,
    path_modified_at,
    path_accessed_at,
    path_is_file,
    path_is_directory,
    path_exists,
    file_open_read_only,
    file_open_write_only,
    file_open_append_only,
    file_open_read_write,
    file_open_read_append,
    file_read,
    directory_create,
    directory_create_recursive,
    directory_remove,
    directory_remove_recursive,
    directory_list
);
