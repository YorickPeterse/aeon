use crate::execution_context::ExecutionContext;
use crate::generator::Generator;
use crate::object_pointer::ObjectPointer;
use crate::object_value;
use crate::process::RcProcess;
use crate::runtime_error::RuntimeError;
use crate::vm::state::RcState;

#[inline(always)]
pub fn allocate(
    state: &RcState,
    process: &RcProcess,
    context: &ExecutionContext,
    block_ptr: ObjectPointer,
    receiver_ptr: ObjectPointer,
    start_reg: u16,
    amount: u16,
) -> Result<ObjectPointer, String> {
    let block = block_ptr.block_value()?;
    let mut new_context =
        ExecutionContext::from_block_with_receiver(&block, receiver_ptr);

    prepare_block_arguments!(context, new_context, start_reg, amount);

    let gen = Generator::created(Box::new(new_context));
    let ptr = process
        .allocate(object_value::generator(gen), state.generator_prototype);

    Ok(ptr)
}

#[inline(always)]
pub fn resume(
    process: &RcProcess,
    gen_ptr: ObjectPointer,
) -> Result<(), String> {
    let gen = gen_ptr.generator_value()?;

    if gen.resume() {
        gen.set_running();
        process.resume_generator(gen.clone());
        Ok(())
    } else {
        Err("Finished generators can't be resumed".to_string())
    }
}

#[inline(always)]
pub fn value(gen_ptr: ObjectPointer) -> Result<ObjectPointer, RuntimeError> {
    let gen = gen_ptr.generator_value()?;

    // If the generator finished or returned early, the process result is
    // written to by the return instruction, but we are only interested in
    // values explicitly yielded.
    if gen.yielded() {
        gen.result().ok_or_else(|| {
            RuntimeError::ErrorMessage(
                "The generator result has already been consumed".to_string(),
            )
        })
    } else {
        // This case is quite common, and we only throw so the standard library
        // can more easily decide what alternative value to produce. As such, we
        // just throw NULL, because we don't use it.
        Err(RuntimeError::Error(ObjectPointer::null()))
    }
}
