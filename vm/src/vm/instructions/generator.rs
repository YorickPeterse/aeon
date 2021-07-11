use crate::execution_context::ExecutionContext;
use crate::indexes::MethodIndex;
use crate::mem::allocator::Pointer;
use crate::mem::generator::{Generator, GeneratorPointer};
use crate::mem::objects::Class;
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::scheduler::process_worker::ProcessWorker;
use crate::vm::state::State;

#[inline(always)]
pub fn allocate(
    state: &State,
    worker: &mut ProcessWorker,
    generator: GeneratorPointer,
    receiver_ptr: Pointer,
    method_idx: u16,
    start_reg: u16,
    amount: u16,
) -> Pointer {
    let method = Class::of(&state.permanent_space, receiver_ptr)
        .get_method(unsafe { MethodIndex::new(method_idx) });

    let mut new_context = ExecutionContext::for_method(method);

    new_context
        .set_registers(generator.context.get_registers(start_reg, amount));

    Generator::alloc(
        worker.allocator(),
        state.permanent_space.generator_class(),
        new_context,
    )
    .as_pointer()
}

#[inline(always)]
pub fn resume(
    mut process: ServerPointer,
    gen_ptr: Pointer,
) -> Result<(), String> {
    let mut gen = unsafe { GeneratorPointer::new(gen_ptr) };

    if !gen.resume() {
        return Err("Finished generators can't be resumed".to_string());
    }

    process.resume_generator(gen);
    Ok(())
}

#[inline(always)]
pub fn value(state: &State, gen_ptr: Pointer) -> Result<Pointer, RuntimeError> {
    let gen = unsafe { GeneratorPointer::new(gen_ptr) };
    let val = gen.result().ok_or_else(|| {
        RuntimeError::ErrorMessage(
            "The generator's result has already been consumed".to_string(),
        )
    })?;

    if gen.yielded() {
        Ok(val)
    } else if gen.thrown() {
        Err(RuntimeError::Error(val))
    } else {
        Ok(state.permanent_space.undefined_singleton)
    }
}
