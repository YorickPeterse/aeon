//! Functions for working with Inko blocks, such as methods and closures.
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{Array, Method, String as InkoString};
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;

/// Returns the name of a method.
///
/// This function requires a single argument: the method to get the data for.
pub fn method_name(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let method = unsafe { arguments[0].get::<Method>() };

    Ok(InkoString::alloc(
        alloc,
        state.permanent_space.string_class(),
        method.name.clone(),
    ))
}

/// Returns the file path of a method.
///
/// This function requires a single argument: the method to get the data for.
pub fn method_file(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let method = unsafe { arguments[0].get::<Method>() };

    Ok(InkoString::alloc(
        alloc,
        state.permanent_space.string_class(),
        method.file.clone(),
    ))
}

/// Returns the line number the method is defined on.
///
/// This function requires a single argument: the method to get the data for.
pub fn method_line(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let method = unsafe { arguments[0].get::<Method>() };

    Ok(Pointer::unsigned_int(method.line as u64))
}

/// Returns the names of the block's arguments.
///
/// This function requires a single argument: the block to get the data for.
pub fn method_arguments(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let method = unsafe { arguments[0].get::<Method>() };
    let str_class = state.permanent_space.string_class();
    let names = method
        .arguments
        .iter()
        .map(|name| InkoString::alloc(alloc, str_class, name.clone()))
        .collect();

    Ok(Array::alloc(
        alloc,
        state.permanent_space.array_class(),
        names,
    ))
}

register!(method_name, method_file, method_line, method_arguments);
