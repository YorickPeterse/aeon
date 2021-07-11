//! VM functions for working with Inko modules.
use crate::mem::allocator::Pointer;
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::String as InkoString;
use crate::vm::state::State;

#[inline(always)]
pub fn load(
    state: &State,
    module_name_ptr: Pointer,
) -> Result<(Pointer, Pointer), String> {
    let name = unsafe { InkoString::read(&module_name_ptr) };
    let (module, exec) =
        state.permanent_space.get_module_for_execution(name)?;

    let exec_ptr = if exec {
        state.permanent_space.true_singleton
    } else {
        state.permanent_space.false_singleton
    };

    Ok((module.as_pointer(), exec_ptr))
}

#[inline(always)]
pub fn current(generator: GeneratorPointer) -> Pointer {
    generator.context.method.module.as_pointer()
}
