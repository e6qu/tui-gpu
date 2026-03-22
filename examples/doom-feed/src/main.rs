use std::{env, ffi::CString, os::raw::c_char};

extern "C" {
    fn doomgeneric_feed_main(argc: i32, argv: *mut *mut c_char) -> i32;
}

fn main() {
    let mut args: Vec<CString> = env::args()
        .map(|arg| CString::new(arg).expect("arg"))
        .collect();
    let mut ptrs: Vec<*mut c_char> = args.iter_mut().map(|c| c.as_ptr() as *mut c_char).collect();
    unsafe {
        doomgeneric_feed_main(ptrs.len() as i32, ptrs.as_mut_ptr());
    }
}
