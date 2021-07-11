//! VM functions for handling external functions.
use crate::external_functions::ExternalFunction;
use crate::mem::allocator::Pointer;
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::String as InkoString;
use crate::mem::process::ServerPointer;
use crate::runtime_error::RuntimeError;
use crate::scheduler::process_worker::ProcessWorker;
use crate::vm::state::State;
use std::mem::transmute;

#[inline(always)]
pub fn call(
    state: &State,
    worker: &mut ProcessWorker,
    process: ServerPointer,
    generator: GeneratorPointer,
    func_ptr: Pointer,
    start_reg: u16,
    amount: u16,
) -> Result<Pointer, RuntimeError> {
    let func: ExternalFunction = unsafe { transmute(func_ptr.as_ptr()) };
    let args = generator.context.get_registers(start_reg, amount);

    func(state, worker.allocator(), process, generator, args)
}

#[inline(always)]
pub fn load(state: &State, name_ptr: Pointer) -> Result<Pointer, String> {
    let name = unsafe { InkoString::read(&name_ptr) };
    let func = state.external_functions.get(name)?;

    Ok(unsafe { Pointer::new(func as *mut u8) })
}
