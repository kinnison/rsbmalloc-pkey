//! pkey interfaces
//!

use libc::{c_int, c_void, size_t};

extern "C" {
    pub fn pkey_mprotect(addr: *mut c_void, len: size_t, prot: c_int, pkey: c_int) -> c_int;
    pub fn pkey_get(pkey: c_int) -> c_int;
    pub fn pkey_set(pkey: c_int, prot: c_int) -> c_int;
}
