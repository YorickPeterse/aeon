use crate::compiled_code::CompiledCodePointer;
use crate::deref_pointer::DerefPointer;
use crate::module::Module;
use crate::object_pointer::ObjectPointer;
use std::sync::atomic::AtomicU8;

/// An enum used to indicate the type of an object.
///
/// The representation and variant values are fixed to ensure they under no
/// circumstance change without this being explicit.
#[repr(u8)]
pub enum Kind {
    // Immix blocks are zeroed out when first created. If the allocator reads an
    // existing chunk of memory, it needs to determine if it's used for storing
    // an object or not. To make this possible, the byte 0 is unused by any real
    // objects.
    Empty = 0,
    Object = 1,
    Integer = 2,
    Float = 3,
}

/// The header used by all managed objects.
///
/// The layout is fixed to ensure we can safely assume certain fields are at
/// certain offsets in an object.
#[repr(C)]
pub struct Header {
    /// The kind of object we're dealing with.
    kind: Kind,

    /// Bits used by the garbage collector, such as when an object needs to be
    /// marked as being forwarded.
    ///
    /// The following bit patterns are used (lowest bit is on the right):
    ///
    /// * 000: the object is a regular object (default)
    /// * 001: this object is in the process of being forwarded
    /// * 010: this object has been forwarded
    /// * 100: this object has been remembered in the remembered set
    gc_bits: AtomicU8,
}

/// A method bound to an object.
#[repr(C)]
pub struct Method {
    /// The code to run when the method is called.
    code: CompiledCodePointer,

    /// The module this method is defined in.
    module: DerefPointer<Module>,
}

/// A virtual method table.
#[repr(C)]
pub struct MethodTable {
    methods: Vec<Method>,
}

#[repr(C)]
pub struct Class {
    header: Header,

    /// The name of the class, as a permanently heap allocated string.
    name: ObjectPointer,

    /// The names of the attributes instances of this class have access to, in
    /// the order they are defined in.
    attributes: Vec<ObjectPointer>,

    /// The static methods of this class.
    static_methods: MethodTable,

    /// The instance methods of this class.
    instance_methods: MethodTable,
}

/// A heap allocated float.
#[repr(C)]
pub struct Float {
    header: Header,
    value: f64,
}

/// A regular object that can store zero or more attributes.
///
/// The size of this object varies based on the number of attributes it has to
/// store.
#[repr(C)]
pub struct Object {
    header: Header,

    // A pointer to the class of this object.
    class: ObjectPointer,

    /// A pointer to the start of the list of attributes. This field _must_ come
    /// last. If there are no attributes, this is a NULL pointer. To access
    /// attribute N, you'd have to calculate the offset of N pointers starting
    /// at this field.
    ///
    /// The amount of space allocated is based on the number of attributes as
    /// defined in this object's class.
    attributes: ObjectPointer,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn test_header_memory_size() {
        assert_eq!(size_of::<Header>(), 2);
    }

    #[test]
    fn test_float_memory_size() {
        assert_eq!(size_of::<Float>(), 16);
    }

    #[test]
    fn test_object_memory_size() {
        assert_eq!(size_of::<Object>(), 24);
    }
}
