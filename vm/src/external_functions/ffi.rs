//! Functions for interacting with C code from Inko.
use crate::ffi::{
    type_alignment, type_size, Function, Library, Pointer as ForeignPointer,
};
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{Array, String as InkoString, UnsignedInt};
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;

/// Loads a C library.
///
/// This function requires one argument: an array of library names to use for
/// loading the library.
pub fn ffi_library_open(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let names = unsafe { arguments[0].get::<Array>() }.value();
    let lib =
        Library::from_pointers(names).map_err(RuntimeError::ErrorMessage)?;

    Ok(Pointer::boxed(lib))
}

/// Loads a C function from a library.
///
/// This function requires the following arguments:
///
/// 1. The libraby to load the function from.
/// 2. The name of the function to load.
/// 3. The types of the function arguments.
/// 4. The return type of the function.
pub fn ffi_function_attach(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let func = unsafe {
        let lib = arguments[0].get::<Library>();
        let name = InkoString::read(&arguments[1]);
        let args = arguments[2].get::<Array>().value();
        let rtype = arguments[3];

        Function::attach(lib, name, args, rtype)?
    };

    Ok(Pointer::boxed(func))
}

/// Calls a C function.
///
/// This function requires the following arguments:
///
/// 1. The function to call.
/// 2. An array containing the function arguments.
pub fn ffi_function_call(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let func = unsafe { arguments[0].get::<Function>() };
    let args = unsafe { arguments[1].get::<Array>() }.value();

    Ok(unsafe { func.call(&state, alloc, args)? })
}

/// Loads a C global variable as pointer.
///
/// This function requires the following arguments:
///
/// 1. The library to load the pointer from.
/// 2. The name of the variable.
pub fn ffi_pointer_attach(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let lib = unsafe { arguments[0].get::<Library>() };
    let name = unsafe { InkoString::read(&arguments[1]) };
    let raw_ptr = unsafe { lib.get(name).map_err(RuntimeError::ErrorMessage)? };

    Ok(unsafe { Pointer::new(raw_ptr.as_ptr()) })
}

/// Returns the value of a pointer.
///
/// This function requires the following arguments:
///
/// 1. The pointer to read from.
/// 2. The type to read the data as.
/// 3. The read offset in bytes.
pub fn ffi_pointer_read(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let ptr = ForeignPointer::new(arguments[0].as_ptr() as _);
    let kind = arguments[1];
    let offset = unsafe { UnsignedInt::read(arguments[2]) as usize };
    let result =
        unsafe { ptr.with_offset(offset).read_as(&state, alloc, kind)? };

    Ok(result)
}

/// Writes a value to a pointer.
///
/// This function requires the following arguments:
///
/// 1. The pointer to write to.
/// 2. The type of data being written.
/// 3. The value to write.
/// 4. The offset to write to.
pub fn ffi_pointer_write(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let ptr = ForeignPointer::new(arguments[0].as_ptr() as _);
    let kind = arguments[1];
    let value = arguments[2];
    let offset = unsafe { UnsignedInt::read(arguments[3]) as usize };

    unsafe {
        ptr.with_offset(offset).write_as(kind, value)?;
    }

    Ok(state.permanent_space.nil_singleton)
}

/// Creates a C pointer from an address.
///
/// This function requires a single argument: the address to use for the
/// pointer.
pub fn ffi_pointer_from_address(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let addr = unsafe { UnsignedInt::read(arguments[0]) };

    Ok(unsafe { Pointer::new(addr as _) })
}

/// Returns the address of a pointer.
///
/// This function requires a single argument: the pointer to get the address of.
pub fn ffi_pointer_address(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let addr = arguments[0].as_ptr() as u64;

    Ok(UnsignedInt::alloc(
        alloc,
        state.permanent_space.unsigned_int_class(),
        addr,
    ))
}

/// Returns the size of an FFI type.
///
/// This function requires a single argument: an integer indicating the FFI
/// type.
pub fn ffi_type_size(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let kind = unsafe { UnsignedInt::read(arguments[0]) };

    type_size(kind).map_err(|e| e.into())
}

/// Returns the alignment of an FFI type.
///
/// This function requires a single argument: an integer indicating the FFI
/// type.
pub fn ffi_type_alignment(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let kind = unsafe { UnsignedInt::read(arguments[0]) };

    type_alignment(kind).map_err(|e| e.into())
}

/// Drops an FFI library.
///
/// This function requires a single argument: the library to drop.
pub fn ffi_library_drop(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe {
        arguments[0].drop_boxed::<Library>();
    }

    Ok(state.permanent_space.nil_singleton)
}

/// Drops an FFI function.
///
/// This function requires a single argument: the function to drop.
pub fn ffi_function_drop(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe {
        arguments[0].drop_boxed::<Function>();
    }

    Ok(state.permanent_space.nil_singleton)
}

register!(
    ffi_library_open,
    ffi_function_attach,
    ffi_function_call,
    ffi_pointer_attach,
    ffi_pointer_read,
    ffi_pointer_write,
    ffi_pointer_from_address,
    ffi_pointer_address,
    ffi_type_size,
    ffi_type_alignment,
    ffi_library_drop,
    ffi_function_drop
);
