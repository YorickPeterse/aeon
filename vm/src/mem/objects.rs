//! Data structures for Inko types, such as classes and methods.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::chunk::Chunk;
use crate::immutable_string::ImmutableString;
use crate::indexes::*;
use crate::mem::allocator::{Allocator, Pointer};
use crate::mem::permanent_space::PermanentSpace;
use crate::mem::process::Server;
use crate::vm::instruction::Instruction;
use std::alloc::{alloc_zeroed, dealloc, handle_alloc_error, Layout};
use std::mem::{align_of, size_of, swap};
use std::ops::Deref;
use std::ptr::drop_in_place;
use std::string::String as RustString;

/// The minimum integer value that can be stored as a tagged signed integer.
pub const MIN_INTEGER: i64 = i64::MIN >> 2;

/// The maximum integer value that can be stored as a tagged signed integer.
pub const MAX_INTEGER: i64 = i64::MAX >> 2;

/// The maximum integer value that can be stored as a tagged unsigned integer.
pub const MAX_UNSIGNED_INTEGER: u64 = u64::MAX >> 2;

/// The header used by types that can have references.
///
/// The layout is fixed to ensure we can safely assume certain fields are at
/// certain offsets in an object, even when not knowing what type of object
/// we're dealing with.
#[repr(C)]
pub struct Header {
    /// The class of the object.
    pub class: ClassPointer,

    /// The number of references still alive.
    ///
    /// 4 294 967 295 references should be enough. If not, what are you doing
    /// that you have that many references?
    pub references: u32,
}

impl Header {
    /// Initialises a header for a bump allocated objects.
    pub fn init(&mut self, class: ClassPointer) {
        self.class = class;
        self.references = 0;
    }
}

/// A method bound to an object.
///
/// Methods are allocated on the permanent heap, and stick around until the
/// program terminates.
#[repr(C)]
pub struct Method {
    header: Header,

    /// The hash of this method, used when performing dynamic dispatch.
    ///
    /// We use a u32 as this is easier to encode into an instruction compared to
    /// a u64.
    pub hash: u32,

    /// The name of the method.
    pub name: RustString,

    /// The path to the source file the method is defined in.
    pub file: RustString,

    /// The line number the method is defined on.
    pub line: u16,

    /// The number of local variables (including arguments) defined.
    pub locals: u16,

    /// The number of registers required.
    pub registers: u16,

    /// The names of the arguments.
    pub arguments: Vec<RustString>,

    /// The instructions to execute.
    pub instructions: Vec<Instruction>,

    /// The module this method is defined in.
    pub module: ModulePointer,
}

impl Method {
    /// Drops the given Method.
    pub fn drop(ptr: MethodPointer) {
        unsafe {
            drop_in_place(ptr.as_pointer().untagged_ptr() as *mut Self);
        }
    }

    /// Returns a new Method.
    pub fn alloc<A: Allocator>(
        alloc: &mut A,
        class: ClassPointer,
        hash: u32,
        name: RustString,
        file: RustString,
        line: u16,
        locals: u16,
        registers: u16,
        arguments: Vec<RustString>,
        instructions: Vec<Instruction>,
        module: ModulePointer,
    ) -> MethodPointer {
        let ptr = alloc.allocate(size_of::<Self>());
        let obj = unsafe { ptr.get_mut::<Self>() };

        obj.header.init(class);

        init!(obj.hash => hash);
        init!(obj.name => name);
        init!(obj.file => file);
        init!(obj.line => line);
        init!(obj.locals => locals);
        init!(obj.registers => registers);
        init!(obj.arguments => arguments);
        init!(obj.instructions => instructions);
        init!(obj.module => module);

        MethodPointer(ptr)
    }

    /// Looks up a method and returns a pointer to it.
    pub fn lookup(
        space: &PermanentSpace,
        receiver: Pointer,
        index: MethodIndex,
    ) -> MethodPointer {
        Class::of(space, receiver).get_method(index)
    }

    /// Looks up a method using a hash and returns a pointer to it.
    pub fn hashed_lookup(
        space: &PermanentSpace,
        receiver: Pointer,
        hash: u32,
    ) -> MethodPointer {
        Class::of(space, receiver).get_hashed_method(hash)
    }

    /// Returns the instruction at the given index, without any bounds checking.
    pub unsafe fn instruction(&self, index: usize) -> &Instruction {
        self.instructions.get_unchecked(index)
    }
}

/// A pointer to an immutable method.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct MethodPointer(Pointer);

impl MethodPointer {
    /// Returns a new MethodPointer from a raw Pointer.
    ///
    /// This method is unsafe as it doesn't perform any checks to ensure the raw
    /// pointer actually points to a method.
    pub unsafe fn new(pointer: Pointer) -> Self {
        Self(pointer)
    }

    /// Returns the MethodPointer as a regular Pointer.
    pub fn as_pointer(self) -> Pointer {
        self.0
    }
}

impl Deref for MethodPointer {
    type Target = Method;

    fn deref(&self) -> &Method {
        unsafe { self.0.get::<Method>() }
    }
}

/// An Inko class.
///
/// Classes come in a variety of sizes, and we don't drop them while the program
/// is running. To make managing memory easier, classes are always allocated
/// using the system allocator.
///
/// Due to the size of this type being variable, it's used/allocated using the
/// Class type, which acts like an owned pointer to this data.
#[repr(C)]
pub struct Class {
    /// The header of this class.
    ///
    /// The class pointer in this header will point to this class itself.
    header: Header,

    /// The name of the class.
    name: RustString,

    /// The names of the fields of this class, in the order they are defined
    /// in.
    fields: Vec<RustString>,

    /// The size (in bytes) of an instance of this class.
    instance_size: usize,

    /// The number of methods in this class.
    methods_len: u16,

    /// All the methods of this class.
    ///
    /// Methods are accessed frequently, and we want to do so with as little
    /// indirection and as cache-friendly as possible. For this reason we use a
    /// flexible array member, instead of a Vec.
    ///
    /// The length of this table is always a power of two, which means some
    /// slots are NULL.
    methods: [MethodPointer; 0],
}

impl Class {
    /// Drops and deallocates the given Class.
    pub fn drop(ptr: ClassPointer) {
        let layout = Self::layout(ptr.methods_len);

        unsafe {
            let raw_ptr = ptr.as_pointer().untagged_ptr();

            drop_in_place(raw_ptr as *mut Self);
            dealloc(raw_ptr, layout);
        }
    }

    /// Allocates a new class and returns a pointer to it.
    ///
    /// Due to their variable size, classes are allocated using the system
    /// allocator.
    pub fn alloc(
        name: RustString,
        fields: Vec<RustString>,
        methods: u16,
        size: usize,
    ) -> ClassPointer {
        let layout = Class::layout(methods);
        let raw_ptr = unsafe { alloc_zeroed(layout) };

        if raw_ptr.is_null() {
            handle_alloc_error(layout);
        }

        let mut class_ptr = unsafe {
            ClassPointer::new(
                Pointer::new(raw_ptr).as_permanent().as_reference(),
            )
        };

        let class_ptr_copy = class_ptr;
        let class = unsafe { class_ptr.get_mut() };

        class.header.init(class_ptr_copy);

        init!(class.name => name);
        init!(class.instance_size => size);
        init!(class.fields => fields);
        init!(class.methods_len => methods);

        class_ptr
    }

    /// Returns a new class for a regular object.
    pub fn object(
        name: RustString,
        fields: Vec<RustString>,
        methods: u16,
    ) -> ClassPointer {
        let size = size_of::<Object>() + (fields.len() * size_of::<Pointer>());

        Self::alloc(name, fields, methods, size)
    }

    /// Returns a new class for a process.
    pub fn process(
        name: RustString,
        fields: Vec<RustString>,
        methods: u16,
    ) -> ClassPointer {
        let size = size_of::<Server>() + (fields.len() * size_of::<Pointer>());

        Self::alloc(name, fields, methods, size)
    }

    /// Returns a pointer to the class of the given pointer.
    pub fn of(space: &PermanentSpace, ptr: Pointer) -> ClassPointer {
        if ptr.is_tagged_int() {
            return space.int_class();
        }

        if ptr.is_tagged_unsigned_int() {
            return space.unsigned_int_class();
        }

        unsafe { ptr.get::<Header>().class }
    }

    /// Returns a Layout to use for allocating a class.
    fn layout(methods: u16) -> Layout {
        let size =
            size_of::<Class>() + (methods as usize * size_of::<Pointer>());

        // Layout errors aren't meant to be handled at runtime (nor can they),
        // so we panic upon failure.
        Layout::from_size_align(size, align_of::<Class>())
            .expect("Failed to create a Class layout")
    }

    pub fn instance_size(&self) -> usize {
        self.instance_size
    }

    pub fn number_of_methods(&self) -> u16 {
        self.methods_len
    }

    pub fn name(&self) -> &RustString {
        &self.name
    }

    pub fn fields(&self) -> &Vec<RustString> {
        &self.fields
    }

    /// Stores a method in a slot.
    pub fn set_method(&mut self, index: MethodIndex, value: MethodPointer) {
        unsafe { self.methods.as_mut_ptr().add(index.into()).write(value) };
    }

    /// Look up a method according to its offset.
    pub fn get_method(&self, index: MethodIndex) -> MethodPointer {
        unsafe { *self.methods.as_ptr().add(index.into()) }
    }

    /// Look up a method according to a hash.
    ///
    /// This method is useful for dynamic dispatch, as an exact offset isn't
    /// known in such cases. For this to work, each unique method name must have
    /// its own unique hash. This method won't work if two different methods
    /// have the same hash.
    ///
    /// In addition, we require that the number of methods in our class is a
    /// power of 2, as this allows the use of a bitwise AND instead of the
    /// modulo operator.
    ///
    /// Finally, similar to `get_method()` we expect there to be a method for
    /// the given hash. In practise this is always the case as the compiler
    /// enforces this, hence we don't check for this explicitly.
    ///
    /// For more information on this technique, refer to
    /// https://thume.ca/2019/07/29/shenanigans-with-hash-tables/.
    pub fn get_hashed_method(&self, input_hash: u32) -> MethodPointer {
        let len = (self.methods_len - 1) as u32;
        let mut hash = input_hash;

        loop {
            hash &= len;

            // The cast to a u16 is safe here, as the above &= ensures we limit
            // the hash value to the method count.
            let ptr = self.get_method(unsafe { MethodIndex::new(hash as u16) });

            if ptr.hash == input_hash {
                return ptr;
            }

            hash += 1;
        }
    }
}

impl Drop for Class {
    fn drop(&mut self) {
        for index in 0..self.number_of_methods() {
            let method = unsafe { self.get_method(MethodIndex::new(index)) };

            if method.as_pointer().as_ptr().is_null() {
                // Because the table size is always a power of two, some slots
                // may be NULL.
                continue;
            }

            Method::drop(method);
        }
    }
}

/// A class that is dropped when this pointer is dropped.
#[repr(transparent)]
pub struct OwnedClass(ClassPointer);

impl OwnedClass {
    pub fn new(ptr: ClassPointer) -> Self {
        Self(ptr)
    }
}

impl Deref for OwnedClass {
    type Target = ClassPointer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for OwnedClass {
    fn drop(&mut self) {
        Class::drop(self.0);
    }
}

/// A pointer to a class.
#[repr(transparent)]
#[derive(Eq, PartialEq, Copy, Clone)]
pub struct ClassPointer(Pointer);

impl ClassPointer {
    /// Returns a new ClassPointer from a raw Pointer.
    ///
    /// This method is unsafe as it doesn't perform any checks to ensure the raw
    /// pointer actually points to a class.
    pub unsafe fn new(pointer: Pointer) -> Self {
        Self(pointer)
    }

    /// Sets a method in the given index.
    pub unsafe fn set_method(
        mut self,
        index: MethodIndex,
        value: MethodPointer,
    ) {
        self.get_mut().set_method(index, value);
    }

    pub fn as_pointer(self) -> Pointer {
        self.0
    }

    /// Returns a mutable reference to the underlying class.
    ///
    /// This method is unsafe because no synchronisation is applied, nor do we
    /// guarantee there's only a single writer.
    unsafe fn get_mut(&mut self) -> &mut Class {
        self.0.get_mut::<Class>()
    }
}

impl Deref for ClassPointer {
    type Target = Class;

    fn deref(&self) -> &Class {
        unsafe { self.0.get::<Class>() }
    }
}

/// A resizable array.
#[repr(C)]
pub struct Array {
    header: Header,
    value: Vec<Pointer>,
}

impl Array {
    /// Drops the given Array.
    ///
    /// This method is unsafe as it doesn't check if the object is actually an
    /// Array.
    pub unsafe fn drop(ptr: Pointer) {
        drop_in_place(ptr.untagged_ptr() as *mut Self);
    }

    /// Bump allocates and initialises a Array.
    pub fn alloc<A: Allocator>(
        allocator: &mut A,
        class: ClassPointer,
        value: Vec<Pointer>,
    ) -> Pointer {
        let size = size_of::<Self>();
        let ptr = allocator.allocate(size);
        let obj = unsafe { ptr.get_mut::<Self>() };

        obj.header.init(class);

        init!(obj.value => value);

        ptr
    }

    pub fn value(&self) -> &Vec<Pointer> {
        &self.value
    }

    pub fn value_mut(&mut self) -> &mut Vec<Pointer> {
        &mut self.value
    }
}

/// A resizable array of bytes.
#[repr(C)]
pub struct ByteArray {
    header: Header,
    value: Vec<u8>,
}

impl ByteArray {
    /// Drops the given ByteArray.
    ///
    /// This method is unsafe as it doesn't check if the object is actually an
    /// ByteArray.
    pub unsafe fn drop(ptr: Pointer) {
        drop_in_place(ptr.untagged_ptr() as *mut Self);
    }

    /// Bump allocates and initialises a ByteArray.
    pub fn alloc<A: Allocator>(
        allocator: &mut A,
        class: ClassPointer,
        value: Vec<u8>,
    ) -> Pointer {
        let size = size_of::<Self>();
        let ptr = allocator.allocate(size);
        let obj = unsafe { ptr.get_mut::<Self>() };

        obj.header.init(class);

        init!(obj.value => value);

        ptr
    }

    pub fn value(&self) -> &Vec<u8> {
        &self.value
    }

    pub fn value_mut(&mut self) -> &mut Vec<u8> {
        &mut self.value
    }

    pub fn take_bytes(&mut self) -> Vec<u8> {
        let mut bytes = Vec::new();

        swap(&mut bytes, &mut self.value);
        bytes
    }
}

/// A signed 64-bits integer.
#[repr(C)]
pub struct Int {
    header: Header,
    value: i64,
}

impl Int {
    /// Returns a new Int.
    ///
    /// If the value is small enough, we use pointer tagging; otherwise the
    /// value is heap allocated.
    pub fn alloc<A: Allocator>(
        alloc: &mut A,
        class: ClassPointer,
        value: i64,
    ) -> Pointer {
        if value >= MIN_INTEGER && value <= MAX_INTEGER {
            return Pointer::int(value);
        }

        let size = size_of::<Self>();
        let ptr = alloc.allocate(size);
        let obj = unsafe { ptr.get_mut::<Self>() };

        obj.header.init(class);

        init!(obj.value => value);

        ptr
    }

    /// Reads the integer value from the pointer.
    ///
    /// If the pointer doesn't actually point to a signed integer, the behaviour
    /// is undefined.
    pub unsafe fn read(ptr: Pointer) -> i64 {
        if ptr.is_tagged_int() {
            ptr.as_int()
        } else {
            ptr.get::<Int>().value
        }
    }
}

/// An unsigned 64-bits integer.
#[repr(C)]
pub struct UnsignedInt {
    header: Header,
    value: u64,
}

impl UnsignedInt {
    /// Returns a new UnsignedInt.
    ///
    /// If the value is small enough, we use pointer tagging; otherwise the
    /// value is heap allocated.
    pub fn alloc<A: Allocator>(
        alloc: &mut A,
        class: ClassPointer,
        value: u64,
    ) -> Pointer {
        if value <= MAX_UNSIGNED_INTEGER {
            return Pointer::unsigned_int(value);
        }

        let size = size_of::<Self>();
        let ptr = alloc.allocate(size);
        let obj = unsafe { ptr.get_mut::<Self>() };

        obj.header.init(class);

        init!(obj.value => value);

        ptr
    }

    /// Reads the integer value from the pointer.
    ///
    /// If the pointer doesn't actually point to an unsigned integer, the
    /// behaviour is undefined.
    pub unsafe fn read(ptr: Pointer) -> u64 {
        if ptr.is_tagged_unsigned_int() {
            ptr.as_unsigned_int()
        } else {
            ptr.get::<UnsignedInt>().value
        }
    }
}

/// A heap allocated float.
#[repr(C)]
pub struct Float {
    header: Header,
    value: f64,
}

impl Float {
    /// Bump allocates and initialises a heap allocated Float.
    pub fn alloc<A: Allocator>(
        allocator: &mut A,
        class: ClassPointer,
        value: f64,
    ) -> Pointer {
        let size = size_of::<Self>();
        let ptr = allocator.allocate(size);
        let obj = unsafe { ptr.get_mut::<Self>() };

        obj.header.init(class);

        init!(obj.value => value);

        ptr
    }

    /// Reads the float value from the pointer.
    ///
    /// If the pointer doesn't actually point to a float, the behaviour is
    /// undefined.
    pub unsafe fn read(ptr: Pointer) -> f64 {
        ptr.get::<Self>().value
    }
}

/// A heap allocated string.
///
/// The string bytes are allocated separately and use atomic reference counting.
/// This makes it cheap to create copies of a string.
#[repr(C)]
pub struct String {
    header: Header,
    value: ArcWithoutWeak<ImmutableString>,
}

impl String {
    /// Drops the given String.
    ///
    /// This method is unsafe as it doesn't check if the object is actually a
    /// String.
    pub unsafe fn drop(ptr: Pointer) {
        drop_in_place(ptr.untagged_ptr() as *mut Self);
    }

    /// Reads a pointer as a String.
    pub unsafe fn read(ptr: &Pointer) -> &str {
        ptr.get::<Self>().value().as_slice()
    }

    /// Returns a new String from a Rust String.
    pub fn alloc<A: Allocator>(
        alloc: &mut A,
        class: ClassPointer,
        value: RustString,
    ) -> Pointer {
        Self::from_immutable_string(
            alloc,
            class,
            ArcWithoutWeak::new(ImmutableString::from(value)),
        )
    }

    /// Returns a new String from an ImmutableString.
    pub fn from_immutable_string<A: Allocator>(
        alloc: &mut A,
        class: ClassPointer,
        value: ArcWithoutWeak<ImmutableString>,
    ) -> Pointer {
        let size = size_of::<Self>();
        let ptr = alloc.allocate(size);
        let obj = unsafe { ptr.get_mut::<Self>() };

        obj.header.init(class);

        init!(obj.value => value);

        ptr
    }

    pub fn value(&self) -> &ArcWithoutWeak<ImmutableString> {
        &self.value
    }
}

/// A module containing classes, methods, and code to run.
#[repr(C)]
pub struct Module {
    header: Header,

    /// The path to the source file that defined the module.
    path: RustString,

    /// The classes defined in this module.
    classes: Vec<OwnedClass>,

    /// The globals this module has access to.
    ///
    /// This includes defined constants, methods and classes, and imported
    /// symbols.
    globals: Chunk<Pointer>,

    /// The literals (e.g. String literals) defined in this module.
    literals: Chunk<Pointer>,

    /// A boolean indicating if this module has been executed or not.
    executed: bool,
}

unsafe impl Send for Module {}

impl Module {
    /// Drops the given Module.
    pub fn drop(ptr: ModulePointer) {
        unsafe {
            drop_in_place(ptr.as_pointer().untagged_ptr() as *mut Self);
        }
    }

    /// Bump allocates and initialises a Module.
    pub fn alloc<A: Allocator>(
        alloc: &mut A,
        class: OwnedClass,
        path: RustString,
        literals: Chunk<Pointer>,
        globals: u16,
    ) -> ModulePointer {
        let size = size_of::<Self>();
        let mut ptr = ModulePointer(alloc.allocate(size));
        let obj = unsafe { ptr.get_mut() };

        obj.header.init(*class);

        init!(obj.path => path);

        // The module owns its own class, so we add it here. This also ensures
        // that dropping the module also drops the module's class.
        init!(obj.classes => vec![class]);
        init!(obj.globals => Chunk::new(globals as usize));
        init!(obj.literals => literals);
        init!(obj.executed => false);

        ptr
    }

    pub fn name(&self) -> &RustString {
        &self.header.class.name
    }

    pub fn source_path(&self) -> &RustString {
        &self.path
    }

    pub fn literals(&self) -> &Chunk<Pointer> {
        &self.literals
    }

    pub fn add_class(&mut self, class: OwnedClass) {
        self.classes.push(class)
    }

    pub fn classes(&self) -> &Vec<OwnedClass> {
        &self.classes
    }

    /// Returns the literal at the given index.
    ///
    /// No bounds checking is performed.
    pub fn get_literal(&self, index: LiteralIndex) -> Pointer {
        unsafe { *self.literals.get(index.into()) }
    }

    /// Marks the module as executed.
    ///
    /// This method assumes some form of synchronisation is used to prevent
    /// concurrent modifications of the same module.
    pub fn mark_as_executed(&mut self) -> bool {
        if self.executed {
            return false;
        }

        self.executed = true;
        true
    }

    /// Returns the value of a global.
    pub fn get_global(&self, index: GlobalIndex) -> Pointer {
        let idx: usize = index.into();

        unsafe { *self.globals.get(idx) }
    }

    /// Sets the value of a global.
    pub fn set_global(&mut self, index: GlobalIndex, value: Pointer) {
        let idx: usize = index.into();

        unsafe {
            self.globals.set(idx, value);
        }
    }

    pub fn class(&self) -> ClassPointer {
        self.header.class
    }
}

/// A module that is dropped when this pointer is dropped.
#[repr(transparent)]
pub struct OwnedModule(ModulePointer);

impl OwnedModule {
    pub fn new(ptr: ModulePointer) -> Self {
        Self(ptr)
    }
}

impl Deref for OwnedModule {
    type Target = ModulePointer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for OwnedModule {
    fn drop(&mut self) {
        Module::drop(self.0);
    }
}

/// A pointer to a module.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct ModulePointer(Pointer);

impl ModulePointer {
    /// Returns a new ModulePointer from a raw Pointer.
    ///
    /// This method is unsafe as it doesn't perform any checks to ensure the raw
    /// pointer actually points to a module.
    pub unsafe fn new(pointer: Pointer) -> Self {
        Self(pointer)
    }

    /// Adds a new class to this module.
    ///
    /// This method is unsafe because we don't perform any locking.
    pub unsafe fn add_class(mut self, class: OwnedClass) {
        self.get_mut().add_class(class);
    }

    /// Marks the underlying module as executed.
    ///
    /// See `Module::mark_as_executed()` for more information.
    pub unsafe fn mark_as_executed(mut self) -> bool {
        self.get_mut().mark_as_executed()
    }

    /// Sets a global variable's value.
    pub unsafe fn set_global(mut self, index: GlobalIndex, value: Pointer) {
        self.get_mut().set_global(index, value);
    }

    pub fn as_pointer(self) -> Pointer {
        self.0
    }

    /// Returns a mutable reference to the underlying module.
    ///
    /// This method is unsafe because no synchronisation is applied, nor do we
    /// guarantee there's only a single writer. It's up to the caller to ensure
    /// they use this method safely.
    unsafe fn get_mut(&mut self) -> &mut Module {
        self.0.get_mut::<Module>()
    }
}

impl Deref for ModulePointer {
    type Target = Module;

    fn deref(&self) -> &Module {
        unsafe { self.0.get::<Module>() }
    }
}

/// A regular object that can store zero or more fields.
///
/// The size of this object varies based on the number of fields it has to
/// store.
#[repr(C)]
pub struct Object {
    header: Header,

    /// The fields of this object.
    ///
    /// The length of this flexible array is derived from the number of
    /// fields defined in this object's class.
    fields: [Pointer; 0],
}

impl Object {
    /// Bump allocates a user-defined object of a variable size.
    ///
    /// This method doesn't write any values to the fields, instead they are
    /// initialised as NULL pointers.
    ///
    /// # Panics
    ///
    /// This method panics if the object to allocate is larger than a single
    /// block. In practise this should never happen, as the compiler and VM
    /// limit the number of fields per class.
    pub fn alloc<A: Allocator>(
        allocator: &mut A,
        class: ClassPointer,
    ) -> Pointer {
        let size = class.instance_size;
        let ptr = allocator.allocate(size);
        let obj = unsafe { ptr.get_mut::<Self>() };

        obj.header.init(class);

        ptr
    }

    /// Assigns a new value to a field.
    pub fn set(&mut self, index: FieldIndex, value: Pointer) {
        unsafe { self.fields.as_mut_ptr().add(index.into()).write(value) };
    }

    /// Returns the value of a field.
    pub fn get(&self, index: FieldIndex) -> Pointer {
        unsafe { *self.fields.as_ptr().add(index.into()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::{bump_allocator, empty_method, empty_module};
    use std::mem::size_of;

    #[test]
    fn test_class_new() {
        let class =
            Class::alloc("A".to_string(), vec!["@a".to_string()], 0, 24);

        assert_eq!(class.fields.len(), 1);
        assert_eq!(class.methods_len, 0);
        assert_eq!(class.instance_size, 24);

        Class::drop(class);
    }

    #[test]
    fn test_class_new_object() {
        let class = Class::object("A".to_string(), vec!["@a".to_string()], 0);

        assert_eq!(class.fields.len(), 1);
        assert_eq!(class.methods_len, 0);
        assert_eq!(class.instance_size, 24);

        Class::drop(class);
    }

    #[test]
    fn test_class_new_process() {
        let class = Class::process("A".to_string(), vec!["@a".to_string()], 0);

        assert_eq!(class.fields.len(), 1);
        assert_eq!(class.methods_len, 0);
        assert_eq!(class.instance_size, 48);

        Class::drop(class);
    }

    #[test]
    fn test_class_methods() {
        let mut alloc = bump_allocator();
        let mod_class = OwnedClass::new(Class::object(
            "foo_mod".to_string(),
            Vec::new(),
            2,
        ));
        let meth_class = Class::object("Method".to_string(), Vec::new(), 0);
        let module = empty_module(&mut alloc, mod_class);
        let foo = empty_method(&mut alloc, meth_class, *module, "foo");
        let bar = empty_method(&mut alloc, meth_class, *module, "bar");
        let index0 = unsafe { MethodIndex::new(0) };
        let index1 = unsafe { MethodIndex::new(1) };

        unsafe {
            module.class().set_method(index0, foo);
            module.class().set_method(index1, bar);
            module.add_class(OwnedClass::new(meth_class));
        }

        assert_eq!(
            module.class().get_method(index0).as_pointer(),
            foo.as_pointer()
        );

        assert_eq!(
            module.class().get_method(index1).as_pointer(),
            bar.as_pointer()
        );
    }

    #[test]
    fn test_module_drop() {
        // This test is a smoke test to make sure no memory is leaked. To verify
        // it actually works, this test should be run using Valgrind or a
        // similar tool.
        let mut alloc = bump_allocator();
        let mod_class = OwnedClass::new(Class::object(
            "foo_mod".to_string(),
            Vec::new(),
            1,
        ));
        let meth_class = Class::object("Method".to_string(), Vec::new(), 0);
        let module = empty_module(&mut alloc, mod_class);
        let method = empty_method(&mut alloc, meth_class, *module, "foo");

        unsafe {
            module.class().set_method(MethodIndex::new(0), method);
            module.add_class(OwnedClass::new(meth_class));
        }
    }

    #[test]
    fn test_memory_sizes() {
        assert_eq!(size_of::<Header>(), 16);
        assert_eq!(size_of::<Object>(), 16); // variable, based on the fields

        assert_eq!(size_of::<Int>(), 24);
        assert_eq!(size_of::<Float>(), 24);
        assert_eq!(size_of::<String>(), 24);

        assert_eq!(size_of::<Array>(), 40);
        assert_eq!(size_of::<ByteArray>(), 40);

        // Permanent objects
        assert_eq!(size_of::<Method>(), 136);
        assert_eq!(size_of::<Class>(), 80);
        assert_eq!(size_of::<Module>(), 104);
    }
}
