//! VM instruction handlers for flow control related operations.
use process::RcProcess;
use vm::instruction::Instruction;
use vm::machine::Machine;

/// Jumps to an instruction if a register is not set or set to false.
///
/// This instruction takes two arguments:
///
/// 1. The instruction index to jump to if a register is not set.
/// 2. The register to check.
#[inline(always)]
pub fn goto_if_false(machine: &Machine,
                     process: &RcProcess,
                     instruction: &Instruction,
                     index: usize)
                     -> usize {
    let go_to = instruction.arg(0);
    let value_reg = instruction.arg(1);

    if is_false!(machine, process.get_register(value_reg)) {
        go_to
    } else {
        index
    }
}

/// Jumps to an instruction if a register is set.
///
/// This instruction takes two arguments:
///
/// 1. The instruction index to jump to if a register is set.
/// 2. The register to check.
#[inline(always)]
pub fn goto_if_true(machine: &Machine,
                    process: &RcProcess,
                    instruction: &Instruction,
                    index: usize)
                    -> usize {
    let go_to = instruction.arg(0);
    let value_reg = instruction.arg(1);

    if is_false!(machine, process.get_register(value_reg)) {
        index
    } else {
        go_to
    }
}

/// Jumps to a specific instruction.
///
/// This instruction takes one argument: the instruction index to jump to.
#[inline(always)]
pub fn goto(instruction: &Instruction) -> usize {
    instruction.arg(0)
}

/// Returns the value in the given register.
///
/// This instruction takes a single argument: the register containing the
/// value to return.
#[inline(always)]
pub fn return_value(process: &RcProcess, instruction: &Instruction) {
    let object = process.get_register(instruction.arg(0));
    let current_context = process.context_mut();

    if let Some(register) = current_context.return_register {
        if let Some(parent_context) = current_context.parent_mut() {
            parent_context.set_register(register, object);
        }
    }
}