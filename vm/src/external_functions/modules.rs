//! Functions for working with Inko modules.
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{Array, Module, String as InkoString};
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;

/// Returns all the modules that have been defined.
pub fn module_list(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let modules = state.permanent_space.list_modules();

    Ok(Array::alloc(
        alloc,
        state.permanent_space.array_class(),
        modules,
    ))
}

/// Returns the name of a module.
///
/// This function requires a single argument: the module to get the data from.
pub fn module_name(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let module = unsafe { arguments[0].get::<Module>() };

    Ok(InkoString::alloc(
        alloc,
        state.permanent_space.string_class(),
        module.name().clone(),
    ))
}

/// Returns the source path of a module.
///
/// This function requires a single argument: the module to get the data from.
pub fn module_source_path(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let module = unsafe { arguments[0].get::<Module>() };

    Ok(InkoString::alloc(
        alloc,
        state.permanent_space.string_class(),
        module.source_path().clone(),
    ))
}

register!(module_list, module_name, module_source_path);
