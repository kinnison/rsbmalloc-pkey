#![feature(allocator_api)]
#![feature(slice_ptr_get)]

use std::{alloc::Allocator, sync::Arc};

use allocator::RSBMalloc;
use libc::c_int;
use pkey::{pkey_alloc, pkey_free, pkey_get, pkey_set, PKEY_DISABLE_ACCESS, PKEY_DISABLE_WRITE};
use static_assertions::assert_impl_all;
use thiserror::Error;

mod allocator;
pub(crate) mod pkey;

#[derive(Clone)]
pub struct ProtectionLabel {
    inner: Arc<ProtectionLabelInner>,
}

struct ProtectionLabelInner {
    label: c_int,
    alloc: RSBMalloc,
}

assert_impl_all!(ProtectionLabelInner: Send, Sync);

#[derive(Debug, Error)]
pub enum ProtectionError {
    #[error("The kernel has run out of protection labels to give to us")]
    OutOfLabels,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ProtectionLevel {
    DenyAll,
    ReadOnly,
    ReadWrite,
}

impl ProtectionLevel {
    fn to_flags(self) -> c_int {
        match self {
            ProtectionLevel::DenyAll => PKEY_DISABLE_ACCESS,
            ProtectionLevel::ReadOnly => PKEY_DISABLE_WRITE,
            ProtectionLevel::ReadWrite => 0,
        }
    }
}

impl ProtectionLabel {
    pub fn create(level: ProtectionLevel) -> Result<Self, ProtectionError> {
        unsafe {
            let label = pkey_alloc(0, 0);
            if label == -1 {
                return Err(ProtectionError::OutOfLabels);
            }
            let alloc = RSBMalloc::new(label);
            let ret = Self {
                inner: Arc::new(ProtectionLabelInner { label, alloc }),
            };
            ret.set_level(level);
            Ok(ret)
        }
    }

    /// # Safety
    ///
    /// It is incumbent upon the caller not to restrict access to this
    /// protection label when unexpected, otherwise segmentation faults
    /// may be induced.
    pub unsafe fn set_level(&self, level: ProtectionLevel) {
        pkey_set(self.inner.label, level.to_flags());
    }

    pub fn with_level<F, O>(&self, level: ProtectionLevel, func: F) -> O
    where
        F: FnOnce(ProtectionLabel) -> O,
    {
        let cur = unsafe {
            let cur = pkey_get(self.inner.label);
            pkey_set(self.inner.label, level.to_flags());
            cur
        };
        let ret = func(self.clone());
        unsafe {
            pkey_set(self.inner.label, cur);
        }
        ret
    }
}

impl Drop for ProtectionLabelInner {
    fn drop(&mut self) {
        unsafe {
            self.alloc.free_all();
            pkey_free(self.label);
        }
    }
}

unsafe impl Allocator for ProtectionLabel {
    fn allocate(
        &self,
        layout: std::alloc::Layout,
    ) -> Result<std::ptr::NonNull<[u8]>, std::alloc::AllocError> {
        self.inner.alloc.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: std::ptr::NonNull<u8>, layout: std::alloc::Layout) {
        self.with_level(ProtectionLevel::ReadWrite, |_| {
            self.inner.alloc.deallocate(ptr, layout)
        })
    }

    fn allocate_zeroed(
        &self,
        layout: std::alloc::Layout,
    ) -> Result<std::ptr::NonNull<[u8]>, std::alloc::AllocError> {
        self.inner.alloc.allocate_zeroed(layout)
    }

    unsafe fn grow(
        &self,
        ptr: std::ptr::NonNull<u8>,
        old_layout: std::alloc::Layout,
        new_layout: std::alloc::Layout,
    ) -> Result<std::ptr::NonNull<[u8]>, std::alloc::AllocError> {
        self.inner.alloc.grow(ptr, old_layout, new_layout)
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: std::ptr::NonNull<u8>,
        old_layout: std::alloc::Layout,
        new_layout: std::alloc::Layout,
    ) -> Result<std::ptr::NonNull<[u8]>, std::alloc::AllocError> {
        self.inner.alloc.grow_zeroed(ptr, old_layout, new_layout)
    }

    unsafe fn shrink(
        &self,
        ptr: std::ptr::NonNull<u8>,
        old_layout: std::alloc::Layout,
        new_layout: std::alloc::Layout,
    ) -> Result<std::ptr::NonNull<[u8]>, std::alloc::AllocError> {
        self.inner.alloc.shrink(ptr, old_layout, new_layout)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn basic_labelled_memory() -> Result<(), ProtectionError> {
        use ProtectionLevel::*;
        let label = ProtectionLabel::create(DenyAll)?;

        let mut sekrit: Vec<i32, _> = label.with_level(ReadWrite, |alloc| {
            let mut v = Vec::new_in(alloc);
            // store my top secret value into this vector
            for i in 0..=1024 {
                v.push(i);
            }
            v
        });

        // You can pass things in for further mutation
        label.with_level(ReadWrite, |_| {
            // Pretend we're decrypting the secret or something
            for v in sekrit.iter_mut() {
                *v = v.wrapping_sub(1024);
            }
        });

        // This would segfault
        //println!("{}", sekrit[0]);
        // As would this
        //label.with_level(ReadOnly, |_| sekrit.push(77));
        // But this doesn't
        label.with_level(ReadOnly, |_| println!("{}", sekrit[0]));

        // Note it's always safe to drop things in protected allocations because
        // the deallocate method will elevate to readwrite internally.
        // It's *NOT* always safe to do anything else with that value though

        Ok(())
    }
}
