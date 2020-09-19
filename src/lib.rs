pub mod connection;
pub mod connection_listener;
pub mod proxychains;

use connection::Connection;
use connection_listener::ConnectionListener;
use futures::{task::Waker, StreamExt};
use libc::{c_int, c_void, size_t, sockaddr, socklen_t, ssize_t};
use proxychains::{Proxy, ProxyChains, ProxyChainsConf};
use std::ffi::CString;
use std::{
    collections::HashMap,
    mem::transmute,
    net::{Ipv4Addr, SocketAddr},
    sync::mpsc::{channel, Sender},
};
#[allow(unused_imports)]
use tokio::prelude::*;
use tokio::{io::copy, runtime::Runtime};

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

// Holds a Connection instance identified by its file descriptor
static mut CONNECTIONS: *mut HashMap<u32, Connection> = 0 as *mut _;

// Waker of ConnectionListener
// Note: Since ConnectionListener is a STREAM of Connections,
// there should be a way to inform it about a new Connection.
// Therefore, in case of new Connection, CONNECTION_LISTENER_WAKER.wake()
// is called.
static mut CONNECTION_LISTENER_WAKER: *mut Waker = 0 as *mut _;

// Sender half of a channel which is responsible to give new Connection(s) to
// ConnectionListener
static mut CONNECTION_SENDER: *mut Sender<(u32, SocketAddr)> = 0 as *mut _;

// Proxychanis Config
static mut CONFIG: *mut ProxyChainsConf = 0 as *mut _;

// Check if Connection to target address exists
unsafe fn exists(sockaddr: SocketAddr) -> bool {
    for (_fd, connection) in (*CONNECTIONS).iter() {
        if connection.target_addr.eq(&sockaddr) {
            return true;
        }
    }
    return false;
}

// Get config
fn config() -> &'static ProxyChainsConf {
    unsafe { &(*CONFIG) }
}

// Get proxies
fn proxies() -> &'static Vec<Proxy> {
    unsafe { &(*CONFIG).proxies }
}

// Check if socket address is a proxy server
// This is required to avoid recursion in connecting to servers
fn is_proxy(addr: &SocketAddr) -> bool {
    proxies().contains(&Proxy {
        socket_addr: *addr,
        auth: None,
    })
}

// Init function: This is run before app starts
#[no_mangle]
#[link_section = ".init_array"]
pub static LD_PRELOAD_INITIALISE_RUST: extern "C" fn() = self::init;
extern "C" fn init() {
    let singleton: HashMap<u32, Connection> = HashMap::new();
    unsafe {
        CONNECTIONS = transmute(Box::new(singleton));
    }

    let (listener_sender, listener_receiver) = channel::<Waker>();

    std::thread::spawn(move || {
        // Initialize config
        let conf = ProxyChainsConf::from_file("config.toml").expect("Failed to prase config file");
        unsafe {
            CONFIG = transmute(Box::new(conf));
        }

        let mut runtime = Runtime::new().unwrap();
        runtime.block_on(async move {
            let (connection_sender, connection_receiver) = channel::<(u32, SocketAddr)>();
            unsafe {
                CONNECTION_SENDER = transmute(Box::new(connection_sender));
            }

            // Create ConnectionListener
            let mut listener = ConnectionListener::new(listener_sender, connection_receiver);

            // Wait for incoming Connections
            while let Some(connection) = listener.next().await {
                let fd = connection.fd;
                unsafe {
                    (*CONNECTIONS).insert(fd, connection);
                }
                tokio::spawn(async move {
                    let connection = unsafe { (*CONNECTIONS).get_mut(&fd) }.unwrap();
                    let target = connection.target_addr.clone();
                    let (connection_reader, connection_writer) = connection.split();

                    let stream = ProxyChains::connect(target, config()).await;

                    if let Ok(mut stream) = stream {
                        let (mut reader, mut writer) = stream.split();
                        let _ = futures::join!(
                            copy(connection_reader, &mut writer),
                            copy(&mut reader, connection_writer)
                        );
                    } else {
                        // LOG: failed to connect proxychains
                    }
                });
            }
        });
    });

    unsafe {
        CONNECTION_LISTENER_WAKER = transmute(Box::new(listener_receiver.recv().unwrap()));
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

    unsafe {
        if !exists(socket_addr) && !is_proxy(&socket_addr) {
            if let Ok(_) = (*CONNECTION_SENDER).send((socket as u32, socket_addr)) {
                (*CONNECTION_LISTENER_WAKER).clone().wake();
            } else {
                // LOG: failed to send new connection info over the cahnnel
            }
        }
    }

    c_connect(socket, address, len)
}

// Hook write function to get outgoing data
#[no_mangle]
fn write(fd: c_int, buf: *const c_void, count: size_t) -> ssize_t {
    let c_write: fn(fd: c_int, buf: *const c_void, count: size_t) -> ssize_t =
        unsafe { transmute(fn_ptr("write")) };

    // Check if write is called with a file descriptor that belongs to a socket connection
    // Prevent the default behavior and redirect data to Connection instance
    if let Some(connection) = unsafe { (*CONNECTIONS).get_mut(&(fd as u32)) } {
        let content: &mut [u8] =
            unsafe { std::slice::from_raw_parts_mut(buf as *mut u8, count as usize) };
        let content: Vec<u8> = Vec::from(content);

        // Redirect data to Connection instance
        if let Some(waker) = connection.get_reader_waker().clone() {
            if let Ok(_) = connection.get_reader_sender().send(content) {
                waker.wake();
            } else {
                // LOG: failed to redirect data to Connection
            }
        }

        count as isize
    } else {
        c_write(fd, buf, count)
    }
}

// Hook read function to fill buffer with incoming data
#[no_mangle]
fn read(fd: c_int, buf: *mut c_void, count: size_t) -> ssize_t {
    let c_read: fn(d: c_int, buf: *const c_void, count: size_t) -> ssize_t =
        unsafe { transmute(fn_ptr("read")) };

    // Check if reading from a socket
    if let Some(connection) = unsafe { (*CONNECTIONS).get_mut(&(fd as u32)) } {
        let buffer: &mut [u8] =
            unsafe { std::slice::from_raw_parts_mut(buf as *mut u8, count as usize) };

        let data = connection.get_writer_receiver().recv().unwrap();
        data.iter().enumerate().for_each(|(i, c)| {
            buffer[i] = *c;
        });

        data.len() as isize
    } else {
        c_read(fd, buf, count)
    }
}
