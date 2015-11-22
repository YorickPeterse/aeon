//! Module for managing memory and prototypes.
//!
//! A MemoryManager can be used to allocate new objects on a heap as well as
//! registering/looking up object prototypes.
//!
//! A MemoryManager struct can be safely shared between threads as any mutable
//! operation uses a read-write lock.

use std::sync::{Arc, RwLock};

use heap::Heap;
use object::{Object, RcObject};
use object_value;
use thread::RcThread;

pub type RcMemoryManager = Arc<RwLock<MemoryManager>>;

/// Structure for managing memory
pub struct MemoryManager {
    /// The latest available object ID.
    pub object_id: usize,

    /// The top-level object used for storing global constants.
    pub top_level: RcObject,

    /// The young heap, most objects will be allocated here.
    pub young_heap: Heap,

    /// The mature heap, used for big objects or those that have outlived
    /// several GC cycles.
    pub mature_heap: Heap,

    pub integer_prototype: RcObject,
    pub float_prototype: RcObject,
    pub string_prototype: RcObject,
    pub array_prototype: RcObject,
    pub thread_prototype: RcObject,
    pub true_prototype: RcObject,
    pub false_prototype: RcObject,
    pub stdout_prototype: RcObject,

    // These are not allocated on any specific heap as they'll never be garbage
    // collected. This also makes retrieving these objects trivial (instead of
    // having to find them somewhere in a heap).
    pub true_object: RcObject,
    pub false_object: RcObject
}

impl MemoryManager {
    pub fn new() -> RcMemoryManager {
        let top_level     = empty_pinned_object!(0);
        let integer_proto = empty_pinned_object!(1);
        let float_proto   = empty_pinned_object!(2);
        let string_proto  = empty_pinned_object!(3);
        let array_proto   = empty_pinned_object!(4);
        let thread_proto  = empty_pinned_object!(5);
        let true_proto    = empty_pinned_object!(6);
        let false_proto   = empty_pinned_object!(7);
        let stdout_proto  = empty_pinned_object!(8);

        let true_obj  = empty_pinned_object!(9);
        let false_obj = empty_pinned_object!(10);

        {
            let mut true_writer  = write_lock!(true_obj);
            let mut false_writer = write_lock!(false_obj);

            true_writer.set_prototype(true_proto.clone());

            false_writer.set_prototype(false_proto.clone());
            false_writer.set_falsy();
        }

        let manager = MemoryManager {
            object_id: 11,
            top_level: top_level,
            young_heap: Heap::new(),
            mature_heap: Heap::new(),
            integer_prototype: integer_proto,
            float_prototype: float_proto,
            string_prototype: string_proto,
            array_prototype: array_proto,
            thread_prototype: thread_proto,
            true_prototype: true_proto,
            false_prototype: false_proto,
            stdout_prototype: stdout_proto,
            true_object: true_obj,
            false_object: false_obj
        };

        Arc::new(RwLock::new(manager))
    }

    /// Creates and allocates a new RcObject.
    pub fn allocate(&mut self, value: object_value::ObjectValue, proto: RcObject) -> RcObject {
        let obj = self.new_object(value);

        write_lock!(obj).set_prototype(proto);

        self.allocate_prepared(obj.clone());

        obj
    }

    /// Allocates an exiting RcObject on the heap.
    pub fn allocate_prepared(&mut self, object: RcObject) {
        self.young_heap.store(object);
    }

    /// Allocates a Thread object based on an existing RcThread.
    pub fn allocate_thread(&mut self, thread: RcThread) -> RcObject {
        let proto      = self.thread_prototype.clone();
        let thread_obj = self.allocate(object_value::thread(thread), proto);

        // Prevent the thread from being GC'd if there are no references to it.
        write_lock!(thread_obj).pin();

        thread_obj
    }

    pub fn integer_prototype(&self) -> RcObject {
        self.integer_prototype.clone()
    }

    pub fn float_prototype(&self) -> RcObject {
        self.float_prototype.clone()
    }

    pub fn string_prototype(&self) -> RcObject {
        self.string_prototype.clone()
    }

    pub fn array_prototype(&self) -> RcObject {
        self.array_prototype.clone()
    }

    pub fn thread_prototype(&self) -> RcObject {
        self.thread_prototype.clone()
    }

    pub fn true_prototype(&self) -> RcObject {
        self.true_prototype.clone()
    }

    pub fn false_prototype(&self) -> RcObject {
        self.false_prototype.clone()
    }

    pub fn stdout_prototype(&self) -> RcObject {
        self.stdout_prototype.clone()
    }

    pub fn true_object(&self) -> RcObject {
        self.true_object.clone()
    }

    pub fn false_object(&self) -> RcObject {
        self.false_object.clone()
    }

    fn new_object_id(&mut self) -> usize {
        self.object_id += 1;

        self.object_id
    }

    pub fn new_object(&mut self, value: object_value::ObjectValue) -> RcObject {
        let obj_id = self.new_object_id();
        let obj    = Object::new(obj_id, value);

        obj
    }
}
