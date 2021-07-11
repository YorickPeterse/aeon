//! Process Execution Contexts
//!
//! An execution context contains the registers, bindings, and other information
//! needed by a process in order to execute bytecode.
use crate::indexes::{GlobalIndex, LocalIndex};
use crate::mem::allocator::Pointer;
use crate::mem::objects::{MethodPointer, ModulePointer};
use crate::registers::Registers;
use crate::variable_scope::VariableScope;
use std::rc::Rc;

/// A single call frame, its variables, registers, and more.
pub struct ExecutionContext {
    /// The local variables this context and any closures within it have access
    /// to.
    pub variables: Rc<VariableScope>,

    /// The code that we're currently running.
    pub method: MethodPointer,

    /// The parent execution context.
    pub parent: Option<Box<ExecutionContext>>,

    /// The index of the instruction to store prior to suspending a process.
    pub index: usize,

    /// The registers for this context.
    registers: Registers,
}

/// Struct for iterating over an ExecutionContext and its parent contexts.
pub struct ExecutionContextIterator<'a> {
    current: Option<&'a ExecutionContext>,
}

impl ExecutionContext {
    pub fn new(code: MethodPointer, variables: Rc<VariableScope>) -> Self {
        ExecutionContext {
            registers: Registers::new(code.registers),
            variables,
            method: code,
            parent: None,
            index: 0,
        }
    }

    /// Returns a new context for a method with the given receiver.
    pub fn for_method(method: MethodPointer) -> Self {
        let variables = VariableScope::new(method.locals);

        ExecutionContext::new(method, variables)
    }

    /// Returns the name of the method that's being executed.
    pub fn name(&self) -> &String {
        &self.method.name
    }

    /// Returns the path of the source file that's being run.
    pub fn file(&self) -> &String {
        &self.method.file
    }

    /// Returns the module the method belongs to.
    pub fn module(&self) -> ModulePointer {
        self.method.module
    }

    /// Returns the line number for the current instruction.
    pub fn line(&self) -> u16 {
        let mut index = self.index;

        // When entering a new call frame, the instruction index stored points
        // to the instruction to run _after_ returning; not the one that is
        // being run.
        if index > 0 {
            index -= 1;
        }

        self.method.instructions[index].line as u16
    }

    /// Returns the value of a single register.
    pub fn get_register(&self, register: u16) -> Pointer {
        self.registers.get(register)
    }

    /// Sets the value of a single register.
    pub fn set_register(&mut self, register: u16, value: Pointer) {
        self.registers.set(register, value);
    }

    /// Returns the values of multiple registers.
    pub fn get_registers(&self, start: u16, amount: u16) -> &[Pointer] {
        self.registers.slice(start, amount)
    }

    /// Sets multiple register values, starting at the first register.
    pub fn set_registers(&mut self, values: &[Pointer]) {
        self.registers.set_slice(values);
    }

    /// Returns the value of a single local variable.
    pub fn get_local(&self, index: LocalIndex) -> Pointer {
        self.variables.get(index)
    }

    /// Sets the value of a single local variable.
    pub fn set_local(&mut self, index: LocalIndex, value: Pointer) {
        self.variables.set(index, value);
    }

    /// Returns the value of a single module-global variable.
    pub fn get_global(&self, index: GlobalIndex) -> Pointer {
        self.method.module.get_global(index)
    }

    /// Sets the value of a single module-global variable.
    ///
    /// This method is unsafe as no synchronisation takes place when updating
    /// the module's data. In addition, it's expected/required that this only
    /// happens once, and when concurrent access to the same variable isn't
    /// possible.
    pub unsafe fn set_global(&mut self, index: GlobalIndex, value: Pointer) {
        self.method.module.set_global(index, value);
    }

    /// Returns an iterator for traversing the context chain, including the
    /// current context.
    pub fn contexts(&self) -> ExecutionContextIterator {
        ExecutionContextIterator {
            current: Some(self),
        }
    }
}

impl<'a> Iterator for ExecutionContextIterator<'a> {
    type Item = &'a ExecutionContext;

    fn next(&mut self) -> Option<&'a ExecutionContext> {
        if let Some(ctx) = self.current {
            if let Some(parent) = ctx.parent.as_ref() {
                self.current = Some(&**parent);
            } else {
                self.current = None;
            }

            return Some(ctx);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::objects::Method;
    use crate::test::{
        bump_allocator, empty_class, empty_method, empty_module,
    };
    use crate::vm::instruction::{Instruction, Opcode};
    use std::mem;

    fn with_context<F: FnOnce(ExecutionContext)>(callback: F) {
        let mut alloc = bump_allocator();
        let class = empty_class("A");
        let module = empty_module(&mut alloc, class);
        let method = empty_method(&mut alloc, module.class(), *module, "foo");
        let ctx = ExecutionContext::for_method(method);

        callback(ctx);

        Method::drop(method);
    }

    #[test]
    fn test_new() {
        let mut alloc = bump_allocator();
        let class = empty_class("A");
        let module = empty_module(&mut alloc, class);
        let method = empty_method(&mut alloc, module.class(), *module, "foo");
        let vars = VariableScope::new(0);
        let ctx = ExecutionContext::new(method, vars);

        assert!(ctx.method.as_pointer() == method.as_pointer());
        assert!(ctx.parent.is_none());
        assert_eq!(ctx.index, 0);

        Method::drop(method);
    }

    #[test]
    fn test_for_method() {
        let mut alloc = bump_allocator();
        let class = empty_class("A");
        let module = empty_module(&mut alloc, class);
        let method = empty_method(&mut alloc, module.class(), *module, "foo");
        let ctx = ExecutionContext::for_method(method);

        assert!(ctx.method.as_pointer() == method.as_pointer());
        assert!(ctx.parent.is_none());
        assert_eq!(ctx.index, 0);

        Method::drop(method);
    }

    #[test]
    fn test_name() {
        with_context(|ctx| {
            assert_eq!(ctx.name(), &"foo".to_string());
        });
    }

    #[test]
    fn test_file() {
        with_context(|ctx| {
            assert_eq!(ctx.file(), &"test.inko".to_string());
        });
    }

    #[test]
    fn test_module() {
        with_context(|ctx| {
            assert_eq!(ctx.module().name(), &"A".to_string());
        });
    }

    #[test]
    fn test_line() {
        with_context(|mut ctx| {
            unsafe {
                let ptr = ctx.method.as_pointer();
                let method = ptr.get_mut::<Method>();

                method.instructions.push(Instruction::new(
                    Opcode::Return,
                    [0; 6],
                    13,
                ));

                method.instructions.push(Instruction::new(
                    Opcode::Return,
                    [0; 6],
                    15,
                ));
            }

            assert_eq!(ctx.line(), 4);

            ctx.index = 2;

            assert_eq!(ctx.line(), 13);
        });
    }

    #[test]
    fn test_get_set_register() {
        with_context(|mut ctx| {
            let ptr = Pointer::int(42);

            ctx.set_register(0, ptr);

            assert_eq!(ctx.get_register(0), ptr);
        });
    }

    #[test]
    fn test_get_set_registers() {
        with_context(|mut ctx| {
            let ptr1 = Pointer::int(42);
            let ptr2 = Pointer::int(43);

            ctx.set_registers(&[ptr1, ptr2]);

            assert_eq!(ctx.get_registers(0, 2), [ptr1, ptr2]);
        });
    }

    #[test]
    fn test_get_set_global() {
        with_context(|mut ctx| unsafe {
            let ptr = Pointer::int(42);
            let idx = GlobalIndex::new(0);

            ctx.set_global(idx, ptr);

            assert_eq!(ctx.get_global(idx), ptr);
        });
    }

    #[test]
    fn test_contexts() {
        with_context(|ctx| {
            let mut iter = ctx.contexts();

            assert!(iter.next().is_some());
            assert!(iter.next().is_none());
        });
    }

    #[test]
    fn test_type_size() {
        assert_eq!(mem::size_of::<ExecutionContext>(), 48);
    }
}
