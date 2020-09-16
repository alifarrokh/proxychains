use libc::{c_int, sockaddr, socklen_t};
use std::ffi::CString;
use std::{
    mem::transmute,
    net::{Ipv4Addr, SocketAddr},
};

// Get function pointer
pub unsafe fn fn_ptr(name: &str) -> *mut core::ffi::c_void {
    libc::dlsym(libc::RTLD_NEXT, CString::new(name).unwrap().into_raw())
}

// Get port number from i8s
pub fn port(left_byte: i8, right_byte: i8) -> u16 {
    ((i8_to_u8(left_byte) as u16) << 8) + i8_to_u8(right_byte) as u16
}

// New IP address from [u8]
pub fn ip(input: &[i8]) -> Ipv4Addr {
    Ipv4Addr::new(
        i8_to_u8(input[0]),
        i8_to_u8(input[1]),
        i8_to_u8(input[2]),
        i8_to_u8(input[3]),
    )
}

// Convert i8 to u8 (with overflow)
pub fn i8_to_u8(n: i8) -> u8 {
    // Note: Not the best way !
    if n == -128i8 {
        128
    } else if n >= 0i8 {
        n as u8
    } else {
        (256 - (-1 * n) as u16) as u8
    }
}

// Hook connect function
#[no_mangle]
fn connect(socket: c_int, address: *const sockaddr, len: socklen_t) -> c_int {
    let c_connect: fn(socket: c_int, address: *const sockaddr, len: socklen_t) -> c_int =
        unsafe { transmute(fn_ptr("connect")) };

    // Obtain socket address
    let sa_data: [i8; 14] = unsafe { (*address).sa_data };
    let socket_addr = SocketAddr::new(ip(&sa_data[2..6]).into(), port(sa_data[0], sa_data[1]));

    println!("Socket Address: {:?}", socket_addr);

    c_connect(socket, address, len)
}
