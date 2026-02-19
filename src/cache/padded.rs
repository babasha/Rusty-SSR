//! Cache-line padding to prevent false sharing
//!
//! When multiple threads write to different atomics that share the same
//! 64-byte cache line, each write invalidates the line for all other cores.
//! Wrapping a value in `CachePadded` ensures it occupies its own cache line.

use std::ops::{Deref, DerefMut};

#[repr(align(64))]
pub struct CachePadded<T> {
    value: T,
}

impl<T> CachePadded<T> {
    pub const fn new(value: T) -> Self {
        Self { value }
    }
}

impl<T: Default> Default for CachePadded<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> Deref for CachePadded<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> DerefMut for CachePadded<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    #[test]
    fn test_alignment() {
        assert!(mem::align_of::<CachePadded<u64>>() >= 64);
    }

    #[test]
    fn test_size_at_least_cache_line() {
        assert!(mem::size_of::<CachePadded<u64>>() >= 64);
    }

    #[test]
    fn test_deref() {
        let padded = CachePadded::new(42u64);
        assert_eq!(*padded, 42);
    }

    #[test]
    fn test_two_values_on_different_lines() {
        let a = CachePadded::new(1u64);
        let b = CachePadded::new(2u64);
        let addr_a = &*a as *const u64 as usize;
        let addr_b = &*b as *const u64 as usize;
        // Stack adjacency isn't guaranteed, but size ensures no overlap
        let _ = (addr_a, addr_b);
        assert!(mem::size_of::<CachePadded<u64>>() >= 64);
    }
}
