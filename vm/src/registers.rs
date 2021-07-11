///! Virtual machine registers
use crate::chunk::Chunk;
use crate::mem::allocator::Pointer;

/// A collection of virtual machine registers.
pub struct Registers {
    pub values: Chunk<Pointer>,
}

impl Registers {
    /// Creates a new Registers.
    pub fn new(amount: u16) -> Registers {
        Registers {
            values: Chunk::new(amount as usize),
        }
    }

    /// Sets the value of the given register.
    pub fn set(&mut self, register: u16, value: Pointer) {
        unsafe { self.values.set(register as usize, value) };
    }

    /// Returns the value of a register.
    pub fn get(&self, register: u16) -> Pointer {
        unsafe { *self.values.get(register as usize) }
    }

    /// Returns a slice containing multiple values, starting at the given index.
    pub fn slice(&self, start: u16, length: u16) -> &[Pointer] {
        unsafe { self.values.slice(start as usize, length as usize) }
    }

    /// Sets the registers starting at index 0 to the values in the slice.
    pub fn set_slice(&mut self, values: &[Pointer]) {
        unsafe {
            self.values
                .slice_mut(0, values.len())
                .copy_from_slice(values);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::allocator::Pointer;

    #[test]
    fn test_set_get() {
        let mut register = Registers::new(6);
        let pointer = unsafe { Pointer::new(0x4 as *mut u8) };

        register.set(0, pointer);
        assert!(register.get(0) == pointer);

        register.set(5, pointer);
        assert!(register.get(5) == pointer);
    }
}
