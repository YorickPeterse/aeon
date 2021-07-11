//! Functions for Inko processes.
use crate::execution_context::ExecutionContext;
use crate::mem::allocator::{BumpAllocator, Pointer};
use crate::mem::generator::GeneratorPointer;
use crate::mem::objects::{Array, Int, String as InkoString};
use crate::mem::process::{Client, ClientPointer, ServerPointer};
use crate::runtime_error::RuntimeError;
use crate::vm::state::State;

/// Returns a stacktrace for the current process.
///
/// This function requires two arguments:
///
/// 1. The number of stack frames to include.
/// 2. The number of stack frames to skip, starting at the current frame.
pub fn process_stacktrace(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    generator: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let limit = unsafe { Int::read(arguments[0]) as usize };
    let skip = unsafe { Int::read(arguments[1]) as usize };

    let mut trace = if limit > 0 {
        Vec::with_capacity(limit)
    } else {
        Vec::new()
    };

    let mut contexts: Vec<&ExecutionContext> = {
        let iter = generator.contexts().into_iter().skip(skip);

        if limit > 0 {
            iter.take(limit).collect()
        } else {
            iter.collect()
        }
    };

    contexts.reverse();

    let str_class = state.permanent_space.string_class();
    let ary_class = state.permanent_space.array_class();

    for context in contexts {
        let file =
            InkoString::alloc(alloc, str_class, context.method.file.clone());
        let name =
            InkoString::alloc(alloc, str_class, context.method.name.clone());
        let line = Pointer::int(i64::from(context.line()));
        let triple = Array::alloc(alloc, ary_class, vec![file, name, line]);

        trace.push(triple);
    }

    Ok(Array::alloc(alloc, ary_class, trace))
}

/// Clones a Client.
///
/// This function requires one argument: the client to clone.
pub fn process_client_clone(
    state: &State,
    alloc: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    let client = unsafe { arguments[0].get::<Client>() };
    let server = client.server();
    let new_client =
        Client::alloc(alloc, state.permanent_space.client_class(), server);

    Ok(new_client.as_pointer())
}

/// Drops a process client.
///
/// This function requires one argument: the client to drop.
pub fn process_client_drop(
    state: &State,
    _: &mut BumpAllocator,
    _: ServerPointer,
    _: GeneratorPointer,
    arguments: &[Pointer],
) -> Result<Pointer, RuntimeError> {
    unsafe { Client::drop(ClientPointer::new(arguments[0])) };

    Ok(state.permanent_space.nil_singleton)
}

register!(
    process_stacktrace,
    process_client_clone,
    process_client_drop
);
