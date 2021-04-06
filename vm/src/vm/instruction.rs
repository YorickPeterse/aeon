//! Structures for encoding virtual machine instructions.

/// Enum containing all possible instruction types.
#[derive(Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    Allocate,
    AllocatePermanent,
    ArrayAllocate,
    ArrayAt,
    ArrayLength,
    ArrayRemove,
    ArraySet,
    AttributeExists,
    BlockGetReceiver,
    ByteArrayAt,
    ByteArrayEquals,
    ByteArrayFromArray,
    ByteArrayLength,
    ByteArrayRemove,
    ByteArraySet,
    Close,
    CopyBlocks,
    CopyRegister,
    Exit,
    ExternalFunctionCall,
    ExternalFunctionLoad,
    FloatAdd,
    FloatDiv,
    FloatEquals,
    FloatGreater,
    FloatGreaterOrEqual,
    FloatMod,
    FloatMul,
    FloatSmaller,
    FloatSmallerOrEqual,
    FloatSub,
    GeneratorAllocate,
    GeneratorResume,
    GeneratorValue,
    GeneratorYield,
    GetAttribute,
    GetAttributeInSelf,
    GetBuiltinPrototype,
    GetFalse,
    GetGlobal,
    GetLocal,
    GetNil,
    GetParentLocal,
    GetPrototype,
    GetTrue,
    Goto,
    GotoIfFalse,
    GotoIfTrue,
    IntegerAdd,
    IntegerBitwiseAnd,
    IntegerBitwiseOr,
    IntegerBitwiseXor,
    IntegerDiv,
    IntegerEquals,
    IntegerGreater,
    IntegerGreaterOrEqual,
    IntegerMod,
    IntegerMul,
    IntegerShiftLeft,
    IntegerShiftRight,
    IntegerSmaller,
    IntegerSmallerOrEqual,
    IntegerSub,
    LocalExists,
    ModuleGet,
    ModuleLoad,
    MoveResult,
    ObjectEquals,
    Panic,
    ProcessAddDeferToCaller,
    ProcessIdentifier,
    ProcessSendMessage,
    ProcessSetBlocking,
    ProcessSetPinned,
    ProcessSpawn,
    ProcessSuspendCurrent,
    ProcessTerminateCurrent,
    Return,
    RunBlock,
    RunBlockWithReceiver,
    SetAttribute,
    SetBlock,
    SetGlobal,
    SetLiteral,
    SetLiteralWide,
    SetLocal,
    SetParentLocal,
    StringByte,
    StringConcat,
    StringEquals,
    StringLength,
    StringSize,
    TailCall,
    Throw,
    FutureGet,
    FutureReady,
    FutureWait,
    IsNull,
}

/// A fixed-width VM instruction.
pub struct Instruction {
    /// The instruction opcode/type.
    pub opcode: Opcode,

    /// The line number of the instruction.
    pub line: u16,

    /// The arguments/operands of the instruction.
    ///
    /// This field is private so other code won't depend on this field having a
    /// particular shape.
    arguments: [u16; 6],
}

impl Instruction {
    pub fn new(opcode: Opcode, arguments: [u16; 6], line: u16) -> Self {
        Instruction {
            opcode,
            arguments,
            line,
        }
    }

    /// Returns the value of the given instruction argument.
    ///
    /// This method is always inlined to ensure bounds checking is optimised
    /// away when using literal index values.
    #[inline(always)]
    pub fn arg(&self, index: usize) -> u16 {
        self.arguments[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    fn new_instruction() -> Instruction {
        Instruction::new(Opcode::SetLiteral, [1, 2, 0, 0, 0, 0], 3)
    }

    #[test]
    fn test_arg() {
        let ins = new_instruction();

        assert_eq!(ins.arg(0), 1);
    }

    #[test]
    fn test_type_size() {
        assert_eq!(size_of::<Instruction>(), 16);
    }
}
