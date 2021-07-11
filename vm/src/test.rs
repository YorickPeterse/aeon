//! Helper functions for writing unit tests.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::chunk::Chunk;
use crate::config::Config;
use crate::mem::allocator::{BlockAllocator, BumpAllocator};
use crate::mem::objects::{
    Class, ClassPointer, Method, MethodPointer, Module, ModulePointer,
    OwnedClass, OwnedModule,
};
use crate::mem::permanent_space::{MethodCounts, PermanentSpace};
use crate::mem::process::{Server, ServerPointer};
use crate::vm::instruction::{Instruction, Opcode};
use crate::vm::state::{RcState, State};
use std::mem::forget;
use std::ops::{Deref, DerefMut, Drop};

/// Servers normally drop themselves when they finish running. But in tests we
/// don't actually run a server.
///
/// To remove the need for manually adding `Server::drop(...)` in every test, we
/// use this wrapper type to automatically drop servers.
pub struct OwnedServer(ServerPointer);

impl OwnedServer {
    pub fn new(server: ServerPointer) -> Self {
        Self(server)
    }

    /// Returns the underlying process, and doesn't run the descructor for this
    /// wrapper.
    pub fn take_and_forget(self) -> ServerPointer {
        let ptr = self.0;

        forget(self);
        ptr
    }
}

impl Deref for OwnedServer {
    type Target = ServerPointer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for OwnedServer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for OwnedServer {
    fn drop(&mut self) {
        Server::drop(self.0);
    }
}

/// Sets up various objects commonly needed in tests, such as an allocator and a
/// dummy process.
///
/// The tuple returns the bump allocater before the state. This way the state is
/// dropped before the allocator. This makes it possible for structures in the
/// state to use objects allocated by the bump allocater, and drop them before
/// the bump allocator is dropped.
pub fn setup() -> (BumpAllocator, RcState, OwnedServer) {
    let config = Config::new();
    let perm = PermanentSpace::new(MethodCounts::default());
    let mut alloc = BumpAllocator::new(perm.blocks.clone());
    let process = Server::alloc(&mut alloc, perm.main_process_class());
    let state = State::new(config, perm, &[]);

    (alloc, state, OwnedServer::new(process))
}

/// Returns a new bump allocator.
pub fn bump_allocator() -> BumpAllocator {
    BumpAllocator::new(ArcWithoutWeak::new(BlockAllocator::new()))
}

/// Allocates and returns a new empty module.
pub fn empty_module(
    alloc: &mut BumpAllocator,
    class: OwnedClass,
) -> OwnedModule {
    OwnedModule::new(Module::alloc(
        alloc,
        class,
        "test.inko".to_string(),
        Chunk::new(0),
        2,
    ))
}

/// Allocates and returns a new empty method.
pub fn empty_method(
    alloc: &mut BumpAllocator,
    class: ClassPointer,
    module: ModulePointer,
    name: &str,
) -> MethodPointer {
    Method::alloc(
        alloc,
        class,
        123,
        name.to_string(),
        "test.inko".to_string(),
        4,
        2,
        2,
        Vec::new(),
        vec![Instruction::new(Opcode::Return, [0; 6], 4)],
        module,
    )
}

/// Allocates an empty class.
pub fn empty_class(name: &str) -> OwnedClass {
    OwnedClass::new(Class::alloc(name.to_string(), Vec::new(), 0, 0))
}

/// Allocates an empty process class.
pub fn empty_process_class(name: &str) -> OwnedClass {
    OwnedClass::new(Class::process(name.to_string(), Vec::new(), 0))
}
