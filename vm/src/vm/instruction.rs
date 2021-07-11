//! Structures for encoding virtual machine instructions.

/// Enum containing all possible instruction types.
#[derive(Debug, PartialEq, Eq)]
#[repr(u8)]
// TODO: IsOwned,
pub enum Opcode {
    Allocate,
    ArrayAllocate,
    ArrayGet,
    ArrayLength,
    ArrayRemove,
    ArraySet,
    ByteArrayAllocate,
    ByteArrayEquals,
    ByteArrayGet,
    ByteArrayLength,
    ByteArrayRemove,
    ByteArraySet,
    CopyRegister,
    DecrementRef,
    Drop,
    DynamicCall,
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
    FutureAllocate,
    FutureGet,
    FutureGetWithTimeout,
    FutureWrite,
    GeneratorAllocate,
    GeneratorResume,
    GeneratorValue,
    GeneratorYield,
    GetClass,
    GetCurrentModule,
    GetFalse,
    GetField,
    GetGlobal,
    GetLiteral,
    GetLiteralWide,
    GetLocal,
    GetNil,
    GetTrue,
    GetUndefined,
    Goto,
    GotoIfFalse,
    GotoIfThrown,
    GotoIfTrue,
    IncrementRef,
    IntAdd,
    IntBitwiseAnd,
    IntBitwiseOr,
    IntBitwiseXor,
    IntDiv,
    IntEquals,
    IntGreater,
    IntGreaterOrEqual,
    IntMod,
    IntMul,
    IntShiftLeft,
    IntShiftRight,
    IntSmaller,
    IntSmallerOrEqual,
    IntSub,
    LoadModule,
    MethodGet,
    MoveResult,
    ObjectEquals,
    Panic,
    ProcessAllocate,
    ProcessSendMessage,
    ProcessSetBlocking,
    ProcessSetPinned,
    ProcessSuspend,
    ProcessYield,
    Return,
    SetField,
    SetGlobal,
    SetLocal,
    StaticCall,
    StringByte,
    StringConcat,
    StringEquals,
    StringLength,
    StringSize,
    Throw,
    UnsignedIntAdd,
    UnsignedIntBitwiseAnd,
    UnsignedIntBitwiseOr,
    UnsignedIntBitwiseXor,
    UnsignedIntDiv,
    UnsignedIntEquals,
    UnsignedIntGreater,
    UnsignedIntGreaterOrEqual,
    UnsignedIntMod,
    UnsignedIntMul,
    UnsignedIntShiftLeft,
    UnsignedIntShiftRight,
    UnsignedIntSmaller,
    UnsignedIntSmallerOrEqual,
    UnsignedIntSub,
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
            line,
            arguments,
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
        Instruction::new(Opcode::GetLiteral, [1, 2, 0, 0, 0, 0], 3)
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
