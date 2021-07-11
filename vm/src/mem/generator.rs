/// Generators/semicoroutines run in a process.
///
/// Generators are a limited form of coroutines, used to make it easier to write
/// iterators. Unlike regular coroutines (sometimes called fibers), generators
/// can only yield to their parent. Once a generator has finished running, it
/// can't be resumed.
use crate::execution_context::ExecutionContext;
use crate::mem::allocator::{Allocator, Pointer};
use crate::mem::objects::{ClassPointer, Header};
use std::mem::{size_of, swap};
use std::ops::{Deref, DerefMut};
use std::ptr::drop_in_place;

/// The status of a generator.
#[derive(PartialEq, Eq, Debug)]
enum Status {
    /// The generator is created but not yet running.
    ///
    /// This is the default state for a generator.
    Created,

    /// The generator is running.
    Running,

    /// The generator yielded a value.
    Yielded,

    /// The generator finished without yielding a value.
    Finished,

    /// The generator threw a value.
    Thrown,
}

/// A generator that can yield to its caller, and be resumed later on.
///
/// A generator is dropped when one of two conditions is met:
///
/// 1. It doesn't have a parent generator, and it finishes running. Such
///    generators are always created by the VM and thus can't exist on the
///    stack.
/// 2. The generator does exist on the stack, and all references to it are
///    dropped.
#[repr(C)]
pub struct Generator {
    header: Header,

    /// The status of the generator.
    status: Status,

    /// The execution context/call stack of this generator.
    pub context: Box<ExecutionContext>,

    /// The parent of this generator, if any.
    pub parent: Option<GeneratorPointer>,

    /// The last value returned, thrown, or yielded.
    result: Option<Pointer>,
}

impl Generator {
    /// Drops the given Generator.
    pub fn drop(ptr: GeneratorPointer) {
        unsafe {
            drop_in_place(ptr.as_pointer().as_ptr() as *mut Self);
        }
    }

    /// Bump allocates and initialises a new generator.
    pub fn alloc<A: Allocator>(
        alloc: &mut A,
        class: ClassPointer,
        context: ExecutionContext,
    ) -> GeneratorPointer {
        let size = size_of::<Self>();
        let mut gen = unsafe { GeneratorPointer::new(alloc.allocate(size)) };

        gen.header.init(class);

        init!(gen.context => Box::new(context));
        init!(gen.parent => None);
        init!(gen.status => Status::Created);
        init!(gen.result => None);

        gen
    }

    /// Adds a new execution context to the stack.
    ///
    /// The parent of the new context is set to the context before the push.
    pub fn push_context(&mut self, new_context: ExecutionContext) {
        let mut boxed = Box::new(new_context);
        let target = &mut self.context;

        swap(target, &mut boxed);
        target.parent = Some(boxed);
    }

    /// Pops the current execution context off the stack.
    ///
    /// If all contexts have been popped, `true` is returned.
    pub fn pop_context(&mut self) -> bool {
        let context = &mut self.context;

        if let Some(parent) = context.parent.take() {
            *context = parent;
            false
        } else {
            // When we're done we need to mark the generator as finished to
            // ensure we don't accidentally resume it again.
            self.finish();
            true
        }
    }

    /// Returns all the contexts of this generator and its parent generators.
    pub fn contexts(&self) -> Vec<&ExecutionContext> {
        let mut contexts = self.context.contexts().collect::<Vec<_>>();
        let mut parent = self.parent.as_ref();

        while let Some(gen) = parent {
            contexts.extend(gen.context.contexts());
            parent = gen.parent.as_ref();
        }

        contexts
    }

    pub fn finish(&mut self) {
        self.status = Status::Finished;
    }

    pub fn yielded(&self) -> bool {
        matches!(self.status, Status::Yielded)
    }

    pub fn thrown(&self) -> bool {
        matches!(self.status, Status::Thrown)
    }

    /// Marks this generator as running again, if possible.
    pub fn resume(&mut self) -> bool {
        match self.status {
            Status::Created | Status::Yielded => {
                self.status = Status::Running;
                true
            }
            _ => false,
        }
    }

    /// Sets the value yielded by this generator.
    ///
    /// A generator can be resumed after it yielded a value.
    pub fn set_yield_value(&mut self, value: Pointer) {
        self.status = Status::Yielded;
        self.result = Some(value);
    }

    /// Sets the value thrown by this generator.
    ///
    /// Once a generator throws, it can't be resumed.
    pub fn set_throw_value(&mut self, value: Pointer) {
        self.result = Some(value);
        self.status = Status::Thrown;
    }

    /// Sets the result of the generator, without changing its status.
    ///
    /// This method is to be used when returning from a method in a generator,
    /// without that return terminating the generator.
    pub fn set_result(&mut self, value: Pointer) {
        self.result = Some(value);
    }

    pub fn take_result(&mut self) -> Option<Pointer> {
        self.result.take()
    }

    pub fn result(&self) -> Option<Pointer> {
        self.result
    }
}

/// A pointer to a generator.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct GeneratorPointer(Pointer);

impl GeneratorPointer {
    /// Returns a new GeneratorPointer from a raw Pointer.
    ///
    /// This method is unsafe as it doesn't perform any checks to ensure the raw
    /// pointer actually points to a generator.
    pub unsafe fn new(pointer: Pointer) -> Self {
        Self(pointer)
    }

    pub fn as_pointer(self) -> Pointer {
        self.0
    }
}

impl Deref for GeneratorPointer {
    type Target = Generator;

    fn deref(&self) -> &Generator {
        unsafe { self.0.get::<Generator>() }
    }
}

impl DerefMut for GeneratorPointer {
    fn deref_mut(&mut self) -> &mut Generator {
        unsafe { self.0.get_mut::<Generator>() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::objects::Method;
    use crate::test::{
        bump_allocator, empty_class, empty_method, empty_module,
    };
    use std::mem::size_of;

    fn with_generator<F>(callback: F)
    where
        F: FnOnce(GeneratorPointer),
    {
        let mut alloc = bump_allocator();
        let module = empty_module(&mut alloc, empty_class("Dummy"));
        let method = empty_method(&mut alloc, module.class(), *module, "foo");
        let ctx = ExecutionContext::for_method(method);
        let gen = Generator::alloc(&mut alloc, module.class(), ctx);

        callback(gen);

        Generator::drop(gen);
        Method::drop(method);
    }

    #[test]
    fn test_generator_new() {
        let mut alloc = bump_allocator();
        let module = empty_module(&mut alloc, empty_class("Dummy"));
        let method = empty_method(&mut alloc, module.class(), *module, "foo");
        let ctx = ExecutionContext::for_method(method);
        let gen = Generator::alloc(&mut alloc, module.class(), ctx);

        assert!(gen.header.class.as_pointer() == module.class().as_pointer());
        assert_eq!(gen.header.references, 0);
        assert_eq!(gen.status, Status::Created);
        assert!(gen.parent.is_none());
        assert!(gen.result.is_none());

        Generator::drop(gen);
        Method::drop(method);
    }

    #[test]
    fn test_generator_push_context() {
        with_generator(|mut gen| {
            let mut ctx2 = ExecutionContext::for_method(gen.context.method);

            ctx2.index = 2;

            gen.push_context(ctx2);

            assert_eq!(gen.context.index, 2);
            assert_eq!(gen.context.parent.as_ref().unwrap().index, 0);
        });
    }

    #[test]
    fn test_generator_pop_context_without_parent() {
        with_generator(|mut gen| {
            assert!(gen.pop_context());
            assert_eq!(gen.status, Status::Finished);
        });
    }

    #[test]
    fn test_generator_pop_context_with_parent() {
        with_generator(|mut gen| {
            let ctx2 = ExecutionContext::for_method(gen.context.method);

            gen.push_context(ctx2);

            assert_eq!(gen.pop_context(), false);
            assert_eq!(gen.status, Status::Created);
            assert_eq!(gen.pop_context(), true);
            assert_eq!(gen.status, Status::Finished);
        });
    }

    #[test]
    fn test_generator_contexts() {
        with_generator(|mut gen| {
            let mut ctx2 = ExecutionContext::for_method(gen.context.method);

            ctx2.index = 4;

            gen.push_context(ctx2);

            let ctxs = gen.contexts();

            assert_eq!(ctxs.len(), 2);
            assert_eq!(ctxs[0].index, 4);
            assert_eq!(ctxs[1].index, 0);
        });
    }

    #[test]
    fn test_generator_finish() {
        with_generator(|mut gen| {
            gen.finish();

            assert_eq!(gen.status, Status::Finished);
        });
    }

    #[test]
    fn test_generator_yielded() {
        with_generator(|mut gen| {
            assert!(!gen.yielded());

            gen.status = Status::Yielded;

            assert!(gen.yielded());
        });
    }

    #[test]
    fn test_generator_thrown() {
        with_generator(|mut gen| {
            assert!(!gen.thrown());

            gen.status = Status::Thrown;

            assert!(gen.thrown());
        });
    }

    #[test]
    fn test_generator_resume() {
        with_generator(|mut gen| {
            assert!(gen.resume());
            assert_eq!(gen.status, Status::Running);

            gen.status = Status::Yielded;

            assert!(gen.resume());
            assert_eq!(gen.status, Status::Running);

            gen.status = Status::Finished;

            assert!(!gen.resume());
            assert_eq!(gen.status, Status::Finished);
        });
    }

    #[test]
    fn test_generator_set_yield_value() {
        with_generator(|mut gen| {
            let value = Pointer::int(42);

            gen.set_yield_value(value);

            assert_eq!(gen.status, Status::Yielded);
            assert_eq!(gen.result, Some(value));
        });
    }

    #[test]
    fn test_generator_set_throw_value() {
        with_generator(|mut gen| {
            let value = Pointer::int(42);

            gen.set_throw_value(value);

            assert_eq!(gen.status, Status::Thrown);
            assert_eq!(gen.result, Some(value));
        });
    }

    #[test]
    fn test_generator_set_result() {
        with_generator(|mut gen| {
            let value = Pointer::int(42);

            gen.set_result(value);

            assert_eq!(gen.status, Status::Created);
            assert_eq!(gen.result, Some(value));
        });
    }

    #[test]
    fn test_generator_take_result() {
        with_generator(|mut gen| {
            let value = Pointer::int(42);

            gen.set_result(value);

            assert_eq!(gen.take_result(), Some(value));
        });
    }

    #[test]
    fn test_generator_result() {
        with_generator(|mut gen| {
            let value = Pointer::int(42);

            assert!(gen.result().is_none());

            gen.set_result(value);

            assert_eq!(gen.result(), Some(value));
        });
    }

    #[test]
    fn test_generator_pointer_as_pointer() {
        let raw_ptr = unsafe { Pointer::new(0x4 as _) };
        let ptr = unsafe { GeneratorPointer::new(raw_ptr) };

        assert_eq!(ptr.as_pointer(), raw_ptr);
    }

    #[test]
    fn test_type_sizes() {
        assert_eq!(size_of::<Generator>(), 48);
    }
}
