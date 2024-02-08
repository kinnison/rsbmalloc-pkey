use core::{
    alloc::{GlobalAlloc, Layout},
    cmp::min,
    mem, ptr,
    ptr::NonNull,
};

use page_allocator::PAGE_ALLOCATOR;
use spin::Mutex;

pub mod page_allocator;

#[cfg(test)]
#[cfg_attr(test, global_allocator)]
static BINNED_ALLOC: RSBMalloc = RSBMalloc::new();

const RSB_CHUNK_SIZE: usize = 0x10000;
const MAX_ALIGN: usize = 0x1000;

pub struct RSBMalloc {
    bins: Bins,
}

impl RSBMalloc {
    pub const fn new() -> Self {
        Self { bins: Bins::new() }
    }
}

unsafe impl GlobalAlloc for RSBMalloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        if layout.align() > MAX_ALIGN {
            return ptr::null_mut();
        }
        let size = layout.pad_to_align().size();
        let bins = &self.bins;
        match size {
            0..=4 => bins.bin4.alloc(),
            5..=8 => bins.bin8.alloc(),
            9..=16 => bins.bin16.alloc(),
            17..=32 => bins.bin32.alloc(),
            33..=64 => bins.bin64.alloc(),
            65..=128 => bins.bin128.alloc(),
            129..=256 => bins.bin256.alloc(),
            257..=512 => bins.bin512.alloc(),
            513..=1024 => bins.bin1024.alloc(),
            1025..=2048 => bins.bin2048.alloc(),
            2049..=4096 => bins.bin4096.alloc(),
            4097..=8192 => bins.bin8192.alloc(),
            8193..=16384 => bins.bin16384.alloc(),
            16385..=0x8000 => bins.bin32ki.alloc(),
            0x8001..=0x10000 => bins.bin64ki.alloc(),
            _ => PAGE_ALLOCATOR.alloc(layout),
        }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        let size = layout.pad_to_align().size();
        let bins = &self.bins;
        match size {
            0..=4 => bins.bin4.dealloc(ptr),
            5..=8 => bins.bin8.dealloc(ptr),
            9..=16 => bins.bin16.dealloc(ptr),
            17..=32 => bins.bin32.dealloc(ptr),
            33..=64 => bins.bin64.dealloc(ptr),
            65..=128 => bins.bin128.dealloc(ptr),
            129..=256 => bins.bin256.dealloc(ptr),
            257..=512 => bins.bin512.dealloc(ptr),
            513..=1024 => bins.bin1024.dealloc(ptr),
            1025..=2048 => bins.bin2048.dealloc(ptr),
            2049..=4096 => bins.bin4096.dealloc(ptr),
            4097..=8192 => bins.bin8192.dealloc(ptr),
            8193..=16384 => bins.bin16384.dealloc(ptr),
            16385..=0x8000 => bins.bin32ki.dealloc(ptr),
            0x8001..=0x10000 => bins.bin64ki.dealloc(ptr),
            _ => PAGE_ALLOCATOR.dealloc(ptr, layout),
        }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if layout.align() > MAX_ALIGN {
            return ptr::null_mut();
        }
        if layout.pad_to_align().size() > RSB_CHUNK_SIZE
            && Layout::from_size_align_unchecked(new_size, layout.align())
                .pad_to_align()
                .size()
                > RSB_CHUNK_SIZE
        {
            return PAGE_ALLOCATOR.realloc(ptr, layout, new_size);
        }
        let new_ptr = self.alloc(Layout::from_size_align_unchecked(new_size, layout.align()));
        if new_ptr != ptr {
            core::ptr::copy_nonoverlapping(ptr, new_ptr, min(layout.size(), new_size));
            self.dealloc(ptr, layout);
        }
        new_ptr
    }
}

#[derive(Default)]
pub(crate) struct Bins {
    pub(crate) bin4: Bin<Slot4>,
    pub(crate) bin8: Bin<Slot8>,
    pub(crate) bin16: Bin<Slot16>,
    pub(crate) bin32: Bin<Slot32>,
    pub(crate) bin64: Bin<Slot64>,
    pub(crate) bin128: Bin<Slot128>,
    pub(crate) bin256: Bin<Slot256>,
    pub(crate) bin512: Bin<Slot512>,
    pub(crate) bin1024: Bin<Slot1024>,
    pub(crate) bin2048: Bin<Slot2048>,
    pub(crate) bin4096: Bin<Slot4096>,
    pub(crate) bin8192: Bin<Slot8192>,
    pub(crate) bin16384: Bin<Slot16384>,
    pub(crate) bin32ki: Bin<Slot32Ki>,
    pub(crate) bin64ki: Bin<Slot64Ki>,
}

impl Bins {
    const fn new() -> Self {
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
}

impl<S: Slot> Default for Bin<S> {
    fn default() -> Self {
        Self {
            free_head: Mutex::new(FreeList::null()),
            page: Mutex::new(Slice {
                ptr: core::ptr::null_mut(),
                len: 0,
            }),
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
    fn add_one(&self) -> *mut S {
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
            let ptr = PAGE_ALLOCATOR.alloc(Layout::from_size_align_unchecked(
                RSB_CHUNK_SIZE,
                mem::align_of::<S>(),
            ));
            let ret = ptr as *mut S;
            page.ptr = ptr.add(slot_size);
            page.len = RSB_CHUNK_SIZE - slot_size;
            ret
        }
    }

    /// Allocates a pointer with size SIZE
    unsafe fn alloc(&self) -> *mut u8 {
        let mut free_head = self.free_head.lock();
        if free_head.exists() {
            let buf = free_head.get_buf();
            (*free_head) = free_head.get_next().into();
            buf
        } else {
            drop(free_head);
            (*self.add_one()).buf()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8) {
        let slot_ptr = ptr as *mut S;
        let mut free_head = self.free_head.lock();
        (*slot_ptr).set_next((*free_head).option_nn());
        (*free_head) = FreeList::from(slot_ptr);
    }

    const fn new() -> Self {
        Self {
            free_head: Mutex::new(FreeList::null()),
            page: Mutex::new(Slice {
                ptr: core::ptr::null_mut(),
                len: 0,
            }),
        }
    }
}

#[cfg(test)]
mod test {
    extern crate alloc;
    extern crate std;
    use core::{
        alloc::{GlobalAlloc, Layout},
        hint::black_box,
        mem,
        ptr::null_mut,
    };

    use std::{panic::catch_unwind, vec, vec::Vec};

    use std::thread;

    use alloc::collections::BTreeMap;

    use crate::*;

    #[repr(align(512))]
    struct Big {
        _contents: [u8; 512],
    }

    unsafe fn test_allocator<A: GlobalAlloc>(allocator: A) {
        std::println!("Allocating 100 i32s");
        let mut pointer = allocator.alloc(Layout::new::<[i32; 100]>());
        assert!(!pointer.is_null());
        let mut slice = std::slice::from_raw_parts_mut(pointer as *mut i32, 100);
        assert_eq!(slice.len(), 100);
        for (i, item) in slice.iter_mut().enumerate() {
            *item = i as i32;
        }
        pointer = allocator.realloc(pointer, Layout::for_value(slice), 4 * 20_000);
        assert!(!pointer.is_null());
        slice = std::slice::from_raw_parts_mut(pointer as *mut i32, 20_000);

        for (i, item) in slice[0..100].iter().enumerate() {
            assert_eq!(*item, i as i32);
        }

        pointer = allocator.realloc(pointer, Layout::for_value(slice), 4 * 50);
        assert!(!pointer.is_null());
        slice = std::slice::from_raw_parts_mut(pointer as *mut i32, 50);
        for (i, item) in slice.iter().enumerate() {
            assert_eq!(*item, i as i32);
        }
        allocator.dealloc(pointer, Layout::for_value(slice));

        // Allocate some more memory
        let ptr = allocator.alloc(Layout::new::<u64>());

        // Free the memory
        allocator.dealloc(ptr, Layout::new::<u64>());

        let mut ptr_buf: [*mut u8; 256] = [null_mut(); 256];

        for ptr in ptr_buf.iter_mut() {
            let pointer = allocator.alloc(Layout::new::<Big>());
            assert!(!pointer.is_null());
            let _ = std::ptr::read(pointer as *const Big);
            *ptr = pointer;
        }

        for ptr in ptr_buf.iter() {
            allocator.dealloc(*ptr, Layout::new::<Big>());
        }

        for ptr in ptr_buf.iter_mut() {
            let pointer = allocator.alloc(Layout::new::<Big>());
            assert!(!pointer.is_null());
            let _ = std::ptr::read(pointer as *const Big);
            *ptr = pointer;
        }

        for ptr in ptr_buf.iter() {
            allocator.dealloc(*ptr, Layout::new::<Big>());
        }
    }

    #[test]
    fn test_page() {
        unsafe {
            test_allocator(crate::page_allocator::PageAllocator {});
        }
    }

    #[test]
    fn align() {
        assert_eq!(mem::align_of::<Slot4>(), 8);
        assert_eq!(mem::align_of::<Slot16>(), 16);
        assert_eq!(mem::align_of::<Slot256>(), 256);
        assert_eq!(mem::align_of::<Slot1024>(), 1024);
    }

    #[test]
    fn test_binned() {
        unsafe { test_allocator(RSBMalloc::new()) };
    }

    #[test]
    fn test_global_allocator() {
        const THREADS: usize = 32;
        const ITERATIONS: usize = 1000;

        let mut map = BTreeMap::new();

        for i in 0..(ITERATIONS) {
            map.insert(format!("Key Nº {}", i), i % 12);
        }

        thread::spawn(move || {
            let _ = map;
        });

        for _ in 0..(ITERATIONS * 100) {
            let vec = vec![0; 256];
            for word in &vec {
                assert_eq!(*word, 0);
            }
            drop(vec);
        }

        let mut threads = Vec::with_capacity(THREADS);

        for i in 0..THREADS {
            threads.push(thread::spawn(move || {
                println!("Starting thread {}", i);
                for _ in 0..ITERATIONS {
                    let mut vec = Vec::with_capacity(0);
                    for _ in 0..513 {
                        vec.push(i);
                    }
                    for byte in vec {
                        assert_eq!(byte, i);
                    }
                }
                println!("Ending thread {}", i);
            }));
        }

        for thread in threads {
            thread.join().unwrap();
        }

        assert!(catch_unwind(|| {
            panic!("Panic!!! Code: {}", black_box(12));
        })
        .is_err());
    }
}
