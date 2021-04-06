/// Generators/semicoroutines run in a process.
///
/// Generators are a limited form of coroutines, used to make it easier to write
/// iterators. Unlike regular coroutines (sometimes called fibers), generators
/// can only yield to their parent. Once a generator has finished running, it
/// can't be resumed.
use crate::execution_context::ExecutionContext;
use crate::object_pointer::{ObjectPointer, ObjectPointerPointer};
use std::cell::UnsafeCell;
use std::mem;
use std::rc::Rc;

enum Status {
    /// The generator is created but not yet running.
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

/// The mutable state of a generator.
struct GeneratorInner {
    /// The execution context/call stack of this generator.
    context: Box<ExecutionContext>,

    /// The parent of this generator, if any.
    parent: Option<RcGenerator>,

    /// The status of the generator.
    ///
    /// A generator can be paused, running, or finished. When a generator is
    /// running, it can't resume itself again, as this can lead to infinite
    /// loops.  In addition, this would lead to reference counting cycles or
    /// break the generator stack. Take this generator stack for example:
    ///
    ///     A -> B -> C
    ///
    /// If C resumes B, we'd end up with the following:
    ///
    ///     A -> B -> C -> B
    ///
    /// Here the B at the tail would have its parent set to C, breaking the
    /// relation between A and B.
    status: Status,

    /// The last result value produced by a return, yield, or throw.
    result: ObjectPointer,
}

/// A generator that can yield to its caller, and be resumed later on.
pub struct Generator {
    /// The mutable state of a generator.
    ///
    /// A generator may be captured and stored in a GC managed object. This
    /// requires reference counting, as at this point there is no clear single
    /// owner. Since Rc types are immutable, we need to use UnsafeCell.
    ///
    /// In practise, the shared ownership is not a problem. Generators aren't
    /// shared across threads, nor does the VM modify them in such a way that
    /// existing mutable references are invalidated.
    inner: UnsafeCell<GeneratorInner>,
}

pub type RcGenerator = Rc<Generator>;

impl Generator {
    fn new(context: Box<ExecutionContext>, status: Status) -> RcGenerator {
        let inner = GeneratorInner {
            context,
            parent: None,
            status,
            result: ObjectPointer::null(),
        };

        Rc::new(Generator {
            inner: UnsafeCell::new(inner),
        })
    }

    pub fn created(context: Box<ExecutionContext>) -> RcGenerator {
        Self::new(context, Status::Created)
    }

    pub fn running(context: Box<ExecutionContext>) -> RcGenerator {
        Self::new(context, Status::Running)
    }

    pub fn push_context(&self, new_context: ExecutionContext) {
        let mut boxed = Box::new(new_context);
        let target = self.context_mut();

        mem::swap(target, &mut boxed);
        target.set_parent(boxed);
    }

    pub fn pop_context(&self) -> bool {
        let context = self.context_mut();

        if let Some(parent) = context.parent.take() {
            *context = parent;
            false
        } else {
            true
        }
    }

    pub fn contexts(&self) -> Vec<&ExecutionContext> {
        let inner = self.inner();
        let mut contexts = inner.context.contexts().collect::<Vec<_>>();
        let mut parent = inner.parent.as_ref();

        while let Some(gen) = parent {
            contexts.extend(gen.context().contexts());
            parent = gen.parent();
        }

        contexts
    }

    pub fn context(&self) -> &ExecutionContext {
        &self.inner().context
    }

    #[cfg_attr(feature = "cargo-clippy", allow(mut_from_ref, borrowed_box))]
    pub fn context_mut(&self) -> &mut Box<ExecutionContext> {
        &mut self.inner_mut().context
    }

    pub fn set_parent(&self, parent: RcGenerator) {
        self.inner_mut().parent = Some(parent);
    }

    pub fn parent(&self) -> Option<&RcGenerator> {
        self.inner().parent.as_ref()
    }

    pub fn take_parent(&self) -> Option<RcGenerator> {
        self.inner_mut().parent.take()
    }

    pub fn set_running(&self) {
        self.inner_mut().status = Status::Running;
    }

    pub fn yielded(&self) -> bool {
        match self.inner().status {
            Status::Yielded => true,
            _ => false,
        }
    }

    pub fn resume(&self) -> bool {
        let inner = self.inner_mut();

        match inner.status {
            Status::Created | Status::Yielded => {
                inner.status = Status::Running;
                true
            }
            _ => false,
        }
    }

    pub fn set_finished(&self) {
        self.inner_mut().status = Status::Finished;
    }

    pub fn yield_value(&self, value: ObjectPointer) {
        let inner = self.inner_mut();

        inner.status = Status::Yielded;
        inner.result = value;
    }

    pub fn set_result(&self, value: ObjectPointer) {
        self.inner_mut().result = value;
    }

    pub fn set_throw_value(&self, value: ObjectPointer) {
        let inner = self.inner_mut();

        inner.result = value;
        inner.status = Status::Thrown;
    }

    pub fn set_return_value(&self, value: ObjectPointer) {
        let inner = self.inner_mut();

        inner.result = value;
        inner.status = Status::Finished;
    }

    pub fn result_thrown(&self) -> bool {
        match self.inner().status {
            Status::Thrown => true,
            _ => false,
        }
    }

    pub fn take_result(&self) -> Option<ObjectPointer> {
        let inner = self.inner_mut();
        let result = inner.result;

        if result.is_null() {
            None
        } else {
            inner.result = ObjectPointer::null();

            Some(result)
        }
    }

    pub fn result(&self) -> Option<ObjectPointer> {
        let result = self.inner().result;

        if result.is_null() {
            None
        } else {
            Some(result)
        }
    }

    pub fn each_pointer<F>(&self, mut callback: F)
    where
        F: FnMut(ObjectPointerPointer),
    {
        if let Some(ptr) = self.result_pointer_pointer() {
            callback(ptr);
        }

        for context in self.context().contexts() {
            context.each_pointer(|v| callback(v));
        }
    }

    pub fn result_pointer_pointer(&self) -> Option<ObjectPointerPointer> {
        let inner = self.inner();

        if inner.result.is_null() {
            None
        } else {
            Some(inner.result.pointer())
        }
    }

    #[cfg_attr(feature = "cargo-clippy", allow(mut_from_ref))]
    fn inner_mut(&self) -> &mut GeneratorInner {
        unsafe { &mut *self.inner.get() }
    }

    fn inner(&self) -> &GeneratorInner {
        unsafe { &*self.inner.get() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn test_generator_inner_type_size() {
        assert_eq!(size_of::<GeneratorInner>(), 32);
    }
}
