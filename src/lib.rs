use libc::c_void;
use std::ffi::CString;
use std::mem::transmute;

#[no_mangle]
fn free(p: *mut c_void) {
    let c_free: fn(p: *mut c_void) = unsafe {
        transmute(libc::dlsym(
            libc::RTLD_NEXT,
            CString::new("free").unwrap().into_raw(),
        ))
    };
    println!("Hello world!");
    c_free(p);
}
