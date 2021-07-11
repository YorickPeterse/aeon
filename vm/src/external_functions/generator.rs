//! Functions for Inko generators.
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::{Generator, GeneratorPointer};
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;

/// Drops a generator.
///
/// This function requires one argument: the generator to drop.
pub fn generator_drop(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let ptr = unsafe { GeneratorPointer::new(arguments[0]) };

    Generator::drop(ptr);
    Ok(state.permanent_space.nil_singleton)
}

register!(generator_drop);
