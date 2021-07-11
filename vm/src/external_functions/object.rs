//! Functions for working with Inko objects.
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{Array, Class, String as InkoString};
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;

/// Returns the names of an object's attributes.
///
/// This function requires one argument: the object to get the attribute names
/// of.
pub fn object_attribute_names(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let class = Class::of(&state.permanent_space, arguments[0]);
    let str_class = state.permanent_space.string_class();
    let names = class
        .fields()
        .iter()
        .map(|name| InkoString::alloc(alloc, str_class, name.clone()))
        .collect();

    Ok(Array::alloc(
        alloc,
        state.permanent_space.array_class(),
        names,
    ))
}

register!(object_attribute_names);
