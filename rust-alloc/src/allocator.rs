use core::alloc::Allocator;
use core::{alloc::Layout, mem, ptr, ptr::NonNull};
use std::alloc::AllocError;

use libc::c_int;
use page_allocator::PageAllocator;
use spin::Mutex;
use static_assertions::assert_impl_all;

mod page_allocator;

const RSB_CHUNK_SIZE: usize = 0x10000;
const MAX_ALIGN: usize = 0x1000;

pub struct RSBMalloc {
    bins: Bins,
    pages: PageAllocator,
}

assert_impl_all!(RSBMalloc: Send, Sync);

impl RSBMalloc {
    /// # Safety
    /// pkey must be a valid protection label
    pub unsafe fn new(pkey: c_int) -> Self {
        Self {
            bins: Bins::new(),
            pages: PageAllocator::new(pkey),
        }
    }

    /// # Safety
    /// Only call this just before releasing the pkey back to the OS
    pub unsafe fn free_all(&self) {
        self.bins.free_all(&self.pages);
    }
}

unsafe impl Allocator for RSBMalloc {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        if layout.align() > MAX_ALIGN {
            return Err(AllocError);
        }
        let size = layout.pad_to_align().size();
        let bins = &self.bins;
        let ptr = unsafe {
            match size {
                0..=4 => bins.bin4.alloc(&self.pages),
                5..=8 => bins.bin8.alloc(&self.pages),
                9..=16 => bins.bin16.alloc(&self.pages),
                17..=32 => bins.bin32.alloc(&self.pages),
                33..=64 => bins.bin64.alloc(&self.pages),
                65..=128 => bins.bin128.alloc(&self.pages),
                129..=256 => bins.bin256.alloc(&self.pages),
                257..=512 => bins.bin512.alloc(&self.pages),
                513..=1024 => bins.bin1024.alloc(&self.pages),
                1025..=2048 => bins.bin2048.alloc(&self.pages),
                2049..=4096 => bins.bin4096.alloc(&self.pages),
                4097..=8192 => bins.bin8192.alloc(&self.pages),
                8193..=16384 => bins.bin16384.alloc(&self.pages),
                16385..=0x8000 => bins.bin32ki.alloc(&self.pages),
                0x8001..=0x10000 => bins.bin64ki.alloc(&self.pages),
                _ => self.pages.alloc(layout),
            }
        };
        let ptr = NonNull::new(ptr).ok_or(AllocError)?;
        Ok(NonNull::slice_from_raw_parts(ptr, size))
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let size = layout.pad_to_align().size();
        let bins = &self.bins;
        let ptr = ptr.as_ptr();
        match size {
            0..=4 => bins.bin4.dealloc(ptr, &self.pages),
            5..=8 => bins.bin8.dealloc(ptr, &self.pages),
            9..=16 => bins.bin16.dealloc(ptr, &self.pages),
            17..=32 => bins.bin32.dealloc(ptr, &self.pages),
            33..=64 => bins.bin64.dealloc(ptr, &self.pages),
            65..=128 => bins.bin128.dealloc(ptr, &self.pages),
            129..=256 => bins.bin256.dealloc(ptr, &self.pages),
            257..=512 => bins.bin512.dealloc(ptr, &self.pages),
            513..=1024 => bins.bin1024.dealloc(ptr, &self.pages),
            1025..=2048 => bins.bin2048.dealloc(ptr, &self.pages),
            2049..=4096 => bins.bin4096.dealloc(ptr, &self.pages),
            4097..=8192 => bins.bin8192.dealloc(ptr, &self.pages),
            8193..=16384 => bins.bin16384.dealloc(ptr, &self.pages),
            16385..=0x8000 => bins.bin32ki.dealloc(ptr, &self.pages),
            0x8001..=0x10000 => bins.bin64ki.dealloc(ptr, &self.pages),
            _ => self.pages.dealloc(ptr, layout),
        }
    }

    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        let ptr = self.allocate(layout)?;
        // SAFETY: `alloc` returns a valid memory block
        unsafe {
            self.pages
                .with_pkey(|| ptr.as_non_null_ptr().as_ptr().write_bytes(0, ptr.len()))
        }
        Ok(ptr)
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "`new_layout.size()` must be greater than or equal to `old_layout.size()`"
        );

        if new_layout.align() > MAX_ALIGN {
            return Err(AllocError);
        }

        if old_layout.pad_to_align().size() > RSB_CHUNK_SIZE {
            let new_ptr = self
                .pages
                .realloc(ptr.as_ptr(), old_layout, new_layout.size());
            let new_ptr = NonNull::new(new_ptr).ok_or(AllocError)?;
            return Ok(NonNull::slice_from_raw_parts(new_ptr, new_layout.size()));
        }

        let new_ptr = self.allocate(new_layout)?;

        // SAFETY: because `new_layout.size()` must be greater than or equal to
        // `old_layout.size()`, both the old and new memory allocation are valid for reads and
        // writes for `old_layout.size()` bytes. Also, because the old allocation wasn't yet
        // deallocated, it cannot overlap `new_ptr`. Thus, the call to `copy_nonoverlapping` is
        // safe. The safety contract for `dealloc` must be upheld by the caller.
        unsafe {
            self.pages.with_pkey(|| {
                ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_mut_ptr(), old_layout.size())
            });
            self.deallocate(ptr, old_layout);
        }

        Ok(new_ptr)
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "`new_layout.size()` must be greater than or equal to `old_layout.size()`"
        );

        if new_layout.align() > MAX_ALIGN {
            return Err(AllocError);
        }

        let new_ptr = self.allocate_zeroed(new_layout)?;

        // SAFETY: because `new_layout.size()` must be greater than or equal to
        // `old_layout.size()`, both the old and new memory allocation are valid for reads and
        // writes for `old_layout.size()` bytes. Also, because the old allocation wasn't yet
        // deallocated, it cannot overlap `new_ptr`. Thus, the call to `copy_nonoverlapping` is
        // safe. The safety contract for `dealloc` must be upheld by the caller.
        unsafe {
            self.pages.with_pkey(|| {
                ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_mut_ptr(), old_layout.size())
            });
            self.deallocate(ptr, old_layout);
        }

        Ok(new_ptr)
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, std::alloc::AllocError> {
        debug_assert!(
            new_layout.size() <= old_layout.size(),
            "`new_layout.size()` must be smaller than or equal to `old_layout.size()`"
        );

        let new_ptr = self.allocate(new_layout)?;

        // SAFETY: because `new_layout.size()` must be lower than or equal to
        // `old_layout.size()`, both the old and new memory allocation are valid for reads and
        // writes for `new_layout.size()` bytes. Also, because the old allocation wasn't yet
        // deallocated, it cannot overlap `new_ptr`. Thus, the call to `copy_nonoverlapping` is
        // safe. The safety contract for `dealloc` must be upheld by the caller.
        unsafe {
            self.pages.with_pkey(|| {
                ptr::copy_nonoverlapping(ptr.as_ptr(), new_ptr.as_mut_ptr(), new_layout.size())
            });
            self.deallocate(ptr, old_layout);
        }

        Ok(new_ptr)
    }
}

#[derive(Default)]
pub(crate) struct Bins {
    bin4: Bin<Slot4>,
    bin8: Bin<Slot8>,
    bin16: Bin<Slot16>,
    bin32: Bin<Slot32>,
    bin64: Bin<Slot64>,
    bin128: Bin<Slot128>,
    bin256: Bin<Slot256>,
    bin512: Bin<Slot512>,
    bin1024: Bin<Slot1024>,
    bin2048: Bin<Slot2048>,
    bin4096: Bin<Slot4096>,
    bin8192: Bin<Slot8192>,
    bin16384: Bin<Slot16384>,
    bin32ki: Bin<Slot32Ki>,
    bin64ki: Bin<Slot64Ki>,
}

impl Bins {
    fn new() -> Self {
        Self {
            bin4: Bin::new(),
            bin8: Bin::new(),
            bin16: Bin::new(),
            bin32: Bin::new(),
            bin64: Bin::new(),
            bin128: Bin::new(),
            bin256: Bin::new(),
            bin512: Bin::new(),
            bin1024: Bin::new(),
            bin2048: Bin::new(),
            bin4096: Bin::new(),
            bin8192: Bin::new(),
            bin16384: Bin::new(),
            bin32ki: Bin::new(),
            bin64ki: Bin::new(),
        }
    }

    fn free_all(&self, pages: &PageAllocator) {
        self.bin4.free_all(pages);
        self.bin8.free_all(pages);
        self.bin16.free_all(pages);
        self.bin32.free_all(pages);
        self.bin64.free_all(pages);
        self.bin128.free_all(pages);
        self.bin256.free_all(pages);
        self.bin512.free_all(pages);
        self.bin1024.free_all(pages);
        self.bin2048.free_all(pages);
        self.bin4096.free_all(pages);
        self.bin8192.free_all(pages);
        self.bin16384.free_all(pages);
        self.bin32ki.free_all(pages);
        self.bin64ki.free_all(pages);
    }
}

pub(crate) trait Slot {
    /// Size is not always the size of the type
    /// For example, a 4 byte size would be valid but the type would be
    /// pointer-sized
    const SIZE: usize;
    unsafe fn buf(&mut self) -> *mut u8;
    unsafe fn next(&self) -> Option<NonNull<Self>>;
    unsafe fn set_next(&mut self, next: Option<NonNull<Self>>);
}

macro_rules! slot {
    ($name:ident, $len:literal, $align:literal) => {
        slot_align!($name, $len, $align);
    };
    ($name:ident, $len:literal) => {
        slot_align!($name, $len, $len);
    };
}

macro_rules! slot_align {
    ($name:ident, $len:literal,$align:literal) => {
        #[repr(align($align))]
        pub(crate) union $name {
            pub(crate) buf: [u8; $len],
            pub(crate) next: Option<NonNull<$name>>,
        }

        impl Slot for $name {
            const SIZE: usize = $len;

            #[inline(always)]
            unsafe fn buf(&mut self) -> *mut u8 {
                &mut self.buf[..] as *mut [u8] as *mut u8
            }

            #[inline(always)]
            unsafe fn next(&self) -> Option<NonNull<$name>> {
                self.next
            }

            #[inline(always)]
            unsafe fn set_next(&mut self, next: Option<NonNull<$name>>) {
                self.next = next;
            }
        }
    };
}

struct Slice {
    ptr: *mut u8,
    len: usize,
}

unsafe impl Send for Slice {}

struct FreeList<S: Slot> {
    ptr: *mut S,
}

unsafe impl<S: Slot> Send for FreeList<S> {}

impl<S: Slot> FreeList<S> {
    fn exists(&self) -> bool {
        !self.ptr.is_null()
    }
    const fn null() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
        }
    }
    unsafe fn get_next(&self) -> Option<NonNull<S>> {
        (*self.ptr).next()
    }
    unsafe fn get_buf(&self) -> *mut u8 {
        (*self.ptr).buf()
    }
    fn option_nn(&self) -> Option<NonNull<S>> {
        NonNull::new(self.ptr)
    }
}

impl<S: Slot> From<Option<NonNull<S>>> for FreeList<S> {
    fn from(value: Option<NonNull<S>>) -> Self {
        Self {
            ptr: match value {
                Some(nn) => nn.as_ptr(),
                None => core::ptr::null_mut(),
            },
        }
    }
}
impl<S: Slot> From<*mut S> for FreeList<S> {
    fn from(value: *mut S) -> Self {
        Self { ptr: value }
    }
}

struct Bin<S: Slot> {
    free_head: Mutex<FreeList<S>>,
    page: Mutex<Slice>,
    pages: Mutex<Vec<(*mut u8, Layout)>>,
}

unsafe impl<S: Slot> Send for Bin<S> {}
unsafe impl<S: Slot> Sync for Bin<S> {}

impl<S: Slot> Default for Bin<S> {
    fn default() -> Self {
        Self {
            free_head: Mutex::new(FreeList::null()),
            page: Mutex::new(Slice {
                ptr: core::ptr::null_mut(),
                len: 0,
            }),
            pages: Mutex::new(Vec::new()),
        }
    }
}

slot!(Slot4, 0x4);
slot!(Slot8, 0x8, 0x4);
slot!(Slot16, 0x10);
slot!(Slot32, 0x20);
slot!(Slot64, 0x40);
slot!(Slot128, 0x80);
slot!(Slot256, 0x100);
slot!(Slot512, 0x200);
slot!(Slot1024, 0x400);
slot!(Slot2048, 0x800);
slot!(Slot4096, 0x1000);
slot!(Slot8192, 0x2000, 0x1000);
slot!(Slot16384, 0x4000, 0x1000);
slot!(Slot32Ki, 0x8000, 0x1000);
slot!(Slot64Ki, 0x10000, 0x1000);

impl<S: Slot> Bin<S> {
    fn add_one(&self, pages: &PageAllocator) -> *mut S {
        let slot_size = mem::size_of::<S>();
        let mut page = self.page.lock();
        if !page.ptr.is_null() && page.len >= slot_size {
            let ret = page.ptr as *mut S;
            unsafe {
                page.ptr = page.ptr.add(slot_size);
                page.len -= slot_size;
            }
            return ret;
        }
        unsafe {
            let layout = Layout::from_size_align_unchecked(RSB_CHUNK_SIZE, mem::align_of::<S>());
            let ptr = pages.alloc(layout);
            self.pages.lock().push((ptr, layout));
            let ret = ptr as *mut S;
            page.ptr = ptr.add(slot_size);
            page.len = RSB_CHUNK_SIZE - slot_size;
            ret
        }
    }

    /// Allocates a pointer with size SIZE
    unsafe fn alloc(&self, pages: &PageAllocator) -> *mut u8 {
        pages.with_pkey(|| {
            let mut free_head = self.free_head.lock();
            if free_head.exists() {
                let buf = free_head.get_buf();
                (*free_head) = free_head.get_next().into();
                buf
            } else {
                drop(free_head);
                (*self.add_one(pages)).buf()
            }
        })
    }

    unsafe fn dealloc(&self, ptr: *mut u8, pages: &PageAllocator) {
        pages.with_pkey(|| {
            let slot_ptr = ptr as *mut S;
            let mut free_head = self.free_head.lock();
            (*slot_ptr).set_next((*free_head).option_nn());
            (*free_head) = FreeList::from(slot_ptr);
        })
    }

    fn new() -> Self {
        Self {
            free_head: Mutex::new(FreeList::null()),
            page: Mutex::new(Slice {
                ptr: core::ptr::null_mut(),
                len: 0,
            }),
            pages: Mutex::new(Vec::new()),
        }
    }

    fn free_all(&self, pages: &PageAllocator) {
        // Note, the order here is important to ensure we don't deadlock
        // though frankly this is part of Drop so we should be fine
        let mut fh = self.free_head.lock();
        let mut p = self.page.lock();
        let mut ps = self.pages.lock();
        *fh = FreeList::null();
        *p = Slice {
            ptr: core::ptr::null_mut(),
            len: 0,
        };
        for (page, layout) in ps.drain(..) {
            unsafe {
                pages.dealloc(page, layout);
            }
        }
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[repr(align(512))]
    struct Big {
        _contents: [u8; 512],
    }

    impl Big {
        fn new() -> Self {
            Self {
                _contents: [0; 512],
            }
        }
    }

    #[test]
    fn basic_vec() {
        let alloc = unsafe { RSBMalloc::new(0) };
        let mut v1 = Vec::new_in(&alloc);
        for i in 0..10_000 {
            v1.push(i);
        }
        let mut v2 = Vec::new_in(&alloc);
        for _ in 0..10_000 {
            v2.push(Big::new());
        }
        drop(v1);
        drop(v2);
        unsafe {
            alloc.free_all();
        }
    }
}
