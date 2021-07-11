//! Functions for setting/getting environment and operating system data.
use crate::directories;
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{Array, String as InkoString};
use crate::mem::process::ServerPointer;
use crate::platform;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;
use std::env;

/// Gets the value of an environment variable.
///
/// This function requires a single argument: the name of the variable to get.
pub fn env_get(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let var_name = unsafe { InkoString::read(&arguments[0]) };
    let result = if let Some(val) = env::var_os(var_name) {
        let string = val.to_string_lossy().into_owned();

        InkoString::alloc(alloc, state.permanent_space.string_class(), string)
    } else {
        state.permanent_space.undefined_singleton
    };

    Ok(result)
}

/// Sets the value of an environment variable.
///
/// This function requires the following arguments:
///
/// 1. The name of the variable to set.
/// 2. The value to set the variable to.
pub fn env_set(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let name = unsafe { InkoString::read(&arguments[0]) };
    let value = unsafe { InkoString::read(&arguments[1]) };

    env::set_var(name, value);
    Ok(state.permanent_space.nil_singleton)
}

/// Removes an environment variable.
///
/// This function requires one argument: the name of the variable to remove.
pub fn env_remove(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let name = unsafe { InkoString::read(&arguments[0]) };

    env::remove_var(name);
    Ok(state.permanent_space.nil_singleton)
}

/// Returns an Array containing all environment variable names.
///
/// This function doesn't take any arguments.
pub fn env_variables(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let names = env::vars_os()
        .map(|(key, _)| {
            let name = key.to_string_lossy().into_owned();

            InkoString::alloc(alloc, state.permanent_space.string_class(), name)
        })
        .collect();

    Ok(Array::alloc(
        alloc,
        state.permanent_space.array_class(),
        names,
    ))
}

/// Returns the user's home directory.
pub fn env_home_directory(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let result = if let Some(path) = directories::home() {
        InkoString::alloc(alloc, state.permanent_space.string_class(), path)
    } else {
        state.permanent_space.undefined_singleton
    };

    Ok(result)
}

/// Returns the temporary directory.
pub fn env_temp_directory(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let res = InkoString::alloc(
        alloc,
        state.permanent_space.string_class(),
        directories::temp(),
    );

    Ok(res)
}

/// Returns the current working directory.
pub fn env_get_working_directory(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = directories::working_directory()?;
    let res =
        InkoString::alloc(alloc, state.permanent_space.string_class(), path);

    Ok(res)
}

/// Sets the working directory.
///
/// This function requires one argument: the path of the new directory.
pub fn env_set_working_directory(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let dir = unsafe { InkoString::read(&arguments[0]) };

    directories::set_working_directory(dir)?;
    Ok(state.permanent_space.nil_singleton)
}

/// Returns the commandline arguments.
pub fn env_arguments(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let res = Array::alloc(
        alloc,
        state.permanent_space.array_class(),
        state.arguments.clone(),
    );

    Ok(res)
}

/// Returns the identifier for the underlying platform.
pub fn env_platform(
    _: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    Ok(Pointer::int(platform::operating_system()))
}

/// Returns the full path to the current executable.
///
/// This function takes no arguments.
pub fn env_executable(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    _: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let path = env::current_exe()?.to_string_lossy().to_string();

    Ok(InkoString::alloc(
        alloc,
        state.permanent_space.string_class(),
        path,
    ))
}

register!(
    env_get,
    env_set,
    env_remove,
    env_variables,
    env_home_directory,
    env_temp_directory,
    env_get_working_directory,
    env_set_working_directory,
    env_arguments,
    env_platform,
    env_executable
);
