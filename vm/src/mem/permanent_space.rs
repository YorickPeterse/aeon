//! Types for memory that stays around permanently.
use crate::arc_without_weak::ArcWithoutWeak;
use crate::chunk::Chunk;
use crate::mem::allocator::{BlockAllocator, PermanentAllocator, Pointer};
use crate::mem::generator::Generator;
use crate::mem::objects::{
    Array, ByteArray, Class, ClassPointer, Float, Int, Method, MethodPointer,
    Module, ModulePointer, Object, OwnedClass, OwnedModule,
    String as InkoString, UnsignedInt,
};
use crate::mem::process::{Client, Future};
use crate::vm::instruction::Instruction;
use ahash::AHashMap;
use parking_lot::Mutex;
use std::mem::size_of;
use std::ops::Drop;

/// Allocates a new class, returning a tuple containing the owned pointer and a
/// permanent reference pointer.
macro_rules! class {
    ($name: expr, $methods: expr, $size_source: ident) => {{
        OwnedClass::new(Class::alloc(
            $name.to_string(),
            Vec::new(),
            $methods,
            size_of::<$size_source>(),
        ))
    }};
}

/// Allocates a singleton object and returns a reference pointer to it.
macro_rules! singleton {
    ($alloc: expr, $class: expr) => {
        Object::alloc(&mut $alloc, *$class).as_reference()
    };
}

/// The number of methods used for the various built-in classes.
///
/// These counts are used to determine how much memory is needed for allocating
/// the various built-in classes.
#[derive(Default)]
pub struct MethodCounts {
    pub int_class: u16,
    pub unsigned_int_class: u16,
    pub float_class: u16,
    pub string_class: u16,
    pub array_class: u16,
    pub method_class: u16,
    pub boolean_class: u16,
    pub nil_class: u16,
    pub byte_array_class: u16,
    pub generator_class: u16,
    pub future_class: u16,
    pub client_class: u16,
}

/// Memory that sticks around for a program's lifetime (aka permanently).
pub struct PermanentSpace {
    /// The class to use for the Int type.
    pub int_class: OwnedClass,

    /// The class to use for the UnsignedInt type.
    pub unsigned_int_class: OwnedClass,

    /// The class to use for the Float type.
    pub float_class: OwnedClass,

    /// The class to use for the String type.
    pub string_class: OwnedClass,

    /// The class to use for the Array type.
    pub array_class: OwnedClass,

    /// The class to use for methods.
    pub method_class: OwnedClass,

    /// The class to use for the Boolean type.
    pub boolean_class: OwnedClass,

    /// The class to use for the NilType type.
    pub nil_class: OwnedClass,

    /// The class to use for the `Undefined` type.
    pub undefined_class: OwnedClass,

    /// The class to use for the ByteArray type.
    pub byte_array_class: OwnedClass,

    /// The class to use for the Generator type.
    pub generator_class: OwnedClass,

    /// The class to use for the Future type.
    pub future_class: OwnedClass,

    /// The class to use for the main process.
    pub main_process_class: OwnedClass,

    /// The class to use for all process clients.
    pub client_class: OwnedClass,

    /// The singleton boolean `True`.
    ///
    /// This (and the other singleton objects) is a proper object instead of a
    /// tagged pointer. This removes the need for extra branches/checks when
    /// looking up methods.
    pub true_singleton: Pointer,

    /// The singleton boolean `False`.
    pub false_singleton: Pointer,

    /// The singleton `Nil` value.
    pub nil_singleton: Pointer,

    /// The singleton `Undefined` value.
    ///
    /// This value is used when we need some kind of sentinel value that user
    /// code can never produce on its own. NULL isn't an option because we don't
    /// allow NULL values in registers or values. Nil won't work either, as user
    /// code can produce Nil values.
    pub undefined_singleton: Pointer,

    /// The allocator to use for requesting new memory blocks.
    pub blocks: ArcWithoutWeak<BlockAllocator>,

    /// All modules that are available to the current program.
    modules: Mutex<AHashMap<String, OwnedModule>>,

    /// The allocator to use for allocating permanent objects.
    allocator: Mutex<PermanentAllocator>,

    /// A map of strings and their heap allocated Inko strings.
    ///
    /// This map is used to ensure that different occurrences of the same string
    /// literal all use the same heap object.
    interned_strings: Mutex<AHashMap<String, Pointer>>,
}

unsafe impl Sync for PermanentSpace {}

impl PermanentSpace {
    /// Returns a new PermanentSpace.
    pub fn new(counts: MethodCounts) -> Self {
        let blocks = ArcWithoutWeak::new(BlockAllocator::new());
        let mut alloc = PermanentAllocator::new(blocks.clone());
        let int_class = class!("Int", counts.int_class, Int);
        let uint_class =
            class!("UnsignedInt", counts.unsigned_int_class, UnsignedInt);
        let float_class = class!("Float", counts.float_class, Float);
        let str_class = class!("String", counts.string_class, InkoString);
        let ary_class = class!("Array", counts.array_class, Array);
        let meth_class = class!("Method", counts.method_class, Method);
        let bool_class = class!("Boolean", counts.boolean_class, Object);
        let nil_class = class!("NilType", counts.nil_class, Object);
        let bary_class =
            class!("ByteArray", counts.byte_array_class, ByteArray);
        let gen_class = class!("Generator", counts.generator_class, Generator);
        let fut_class = class!("Future", counts.future_class, Future);
        let client_class = class!("Client", counts.client_class, Client);

        // The main process' class doesn't support any methods (async or not),
        // so we don't allocate space for them.
        let main_proc_class =
            OwnedClass::new(Class::process("Main".to_string(), Vec::new(), 0));

        // The UndefinedType class isn't accessible to the runtime, so it too
        // doesn't support/need any methods.
        let undef_class = class!("UndefinedType", 0, Object);

        let true_singleton = singleton!(alloc, bool_class);
        let false_singleton = singleton!(alloc, bool_class);
        let nil_singleton = singleton!(alloc, nil_class);
        let undefined_singleton = singleton!(alloc, undef_class);

        Self {
            int_class,
            unsigned_int_class: uint_class,
            float_class,
            string_class: str_class,
            array_class: ary_class,
            method_class: meth_class,
            boolean_class: bool_class,
            nil_class,
            byte_array_class: bary_class,
            generator_class: gen_class,
            future_class: fut_class,
            main_process_class: main_proc_class,
            undefined_class: undef_class,
            client_class,
            true_singleton,
            false_singleton,
            nil_singleton,
            undefined_singleton,
            blocks,
            allocator: Mutex::new(alloc),
            interned_strings: Mutex::new(AHashMap::default()),
            modules: Mutex::new(AHashMap::default()),
        }
    }

    pub fn int_class(&self) -> ClassPointer {
        *self.int_class
    }

    pub fn unsigned_int_class(&self) -> ClassPointer {
        *self.unsigned_int_class
    }

    pub fn float_class(&self) -> ClassPointer {
        *self.float_class
    }

    pub fn string_class(&self) -> ClassPointer {
        *self.string_class
    }

    pub fn array_class(&self) -> ClassPointer {
        *self.array_class
    }

    pub fn method_class(&self) -> ClassPointer {
        *self.method_class
    }

    pub fn boolean_class(&self) -> ClassPointer {
        *self.boolean_class
    }

    pub fn nil_class(&self) -> ClassPointer {
        *self.nil_class
    }

    pub fn byte_array_class(&self) -> ClassPointer {
        *self.byte_array_class
    }

    pub fn generator_class(&self) -> ClassPointer {
        *self.generator_class
    }

    pub fn future_class(&self) -> ClassPointer {
        *self.future_class
    }

    pub fn main_process_class(&self) -> ClassPointer {
        *self.main_process_class
    }

    pub fn client_class(&self) -> ClassPointer {
        *self.client_class
    }

    pub fn undefined_class(&self) -> ClassPointer {
        *self.undefined_class
    }

    /// Interns a permanent string.
    ///
    /// If an Inko String has already been allocated for the given Rust String,
    /// the existing Inko String is returned; otherwise a new one is created.
    pub fn allocate_string(&self, string: String) -> Pointer {
        let mut strings = self.interned_strings.lock();

        if let Some(ptr) = strings.get(&string) {
            return *ptr;
        }

        let mut alloc = self.allocator.lock();
        let pointer =
            InkoString::alloc(&mut *alloc, self.string_class(), string.clone());

        strings.insert(string, pointer);
        pointer
    }

    pub fn allocate_int(&self, value: i64) -> Pointer {
        let mut alloc = self.allocator.lock();

        Int::alloc(&mut *alloc, self.int_class(), value)
    }

    pub fn allocate_unsigned_int(&self, value: u64) -> Pointer {
        let mut alloc = self.allocator.lock();

        UnsignedInt::alloc(&mut *alloc, self.int_class(), value)
    }

    pub fn allocate_float(&self, value: f64) -> Pointer {
        let mut alloc = self.allocator.lock();

        Float::alloc(&mut *alloc, self.float_class(), value)
    }

    pub fn allocate_method(
        &self,
        hash: u32,
        name: String,
        file: String,
        line: u16,
        locals: u16,
        registers: u16,
        arguments: Vec<String>,
        instructions: Vec<Instruction>,
        module: ModulePointer,
    ) -> MethodPointer {
        let mut alloc = self.allocator.lock();

        Method::alloc(
            &mut *alloc,
            self.method_class(),
            hash,
            name,
            file,
            line,
            locals,
            registers,
            arguments,
            instructions,
            module,
        )
    }

    /// Allocates a module using the permanent allocator.
    ///
    /// When a module is first allocated, its class is set to NULL. This is due
    /// to the module's class being derived from the module itself, and because
    /// the methods in a module depend on the module.
    ///
    /// To break this circular dependency, the class must be corrected after
    /// allocating a module and processing its methods.
    pub fn allocate_module(
        &self,
        class: OwnedClass,
        path: String,
        literals: Chunk<Pointer>,
        globals: u16,
    ) -> OwnedModule {
        let mut alloc = self.allocator.lock();
        let ptr = Module::alloc(&mut *alloc, class, path, literals, globals);

        OwnedModule::new(ptr)
    }

    pub fn add_modules(&self, modules: Vec<OwnedModule>) {
        let mut mapping = self.modules.lock();

        for ptr in modules {
            mapping.insert(ptr.name().clone(), ptr);
        }
    }

    /// Returns a module for the given name.
    ///
    /// The name must be the full module name, including its namespace (so
    /// `std::string` and not just `string`).
    pub fn get_module(&self, name: &str) -> Result<ModulePointer, String> {
        self.modules
            .lock()
            .get(name)
            .map(|x| **x)
            .ok_or_else(|| format!("The module {} doesn't exist", name))
    }

    /// Returns all the modules that have been defined.
    pub fn list_modules(&self) -> Vec<Pointer> {
        self.modules
            .lock()
            .values()
            .map(|x| (*x).as_pointer())
            .collect()
    }

    /// Returns a module to execute.
    ///
    /// The returned tuple contains the module, and a boolean indicating if the
    /// module can be executed.
    pub fn get_module_for_execution(
        &self,
        name: &str,
    ) -> Result<(ModulePointer, bool), String> {
        // We don't reuse get_module() here because we need to retain the lock
        // _until_ we have marked a module as executed (if not already done so).
        let modules = self.modules.lock();

        if let Some(ptr) = modules.get(name) {
            return Ok((**ptr, unsafe { ptr.mark_as_executed() }));
        }

        Err(format!("The module {} doesn't exist", name))
    }
}

impl Drop for PermanentSpace {
    fn drop(&mut self) {
        for pointer in self.interned_strings.lock().values() {
            unsafe {
                InkoString::drop(*pointer);
            }
        }

        // The singleton objects can't contain any sub values, so they don't
        // need to be dropped explicitly.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::generator::Generator;
    use crate::mem::objects::{
        Array, ByteArray, Float, Int, Method, Object, String as InkoString,
        UnsignedInt,
    };
    use crate::mem::process::{Client, Future, Server};
    use std::mem::size_of;

    #[test]
    fn test_class_instance_sizes() {
        let perm = PermanentSpace::new(MethodCounts::default());

        assert_eq!(perm.int_class.instance_size(), size_of::<Int>());
        assert_eq!(
            perm.unsigned_int_class.instance_size(),
            size_of::<UnsignedInt>()
        );
        assert_eq!(perm.float_class.instance_size(), size_of::<Float>());
        assert_eq!(perm.string_class.instance_size(), size_of::<InkoString>());
        assert_eq!(perm.array_class.instance_size(), size_of::<Array>());
        assert_eq!(perm.method_class.instance_size(), size_of::<Method>());
        assert_eq!(perm.boolean_class.instance_size(), size_of::<Object>());
        assert_eq!(perm.nil_class.instance_size(), size_of::<Object>());
        assert_eq!(perm.undefined_class.instance_size(), size_of::<Object>());
        assert_eq!(
            perm.byte_array_class.instance_size(),
            size_of::<ByteArray>()
        );
        assert_eq!(
            perm.generator_class.instance_size(),
            size_of::<Generator>()
        );
        assert_eq!(perm.future_class.instance_size(), size_of::<Future>());
        assert_eq!(
            perm.main_process_class.instance_size(),
            size_of::<Server>()
        );
        assert_eq!(perm.client_class.instance_size(), size_of::<Client>());
    }
}
