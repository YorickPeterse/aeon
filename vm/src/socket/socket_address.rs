use socket2::SockAddr;

#[cfg(unix)]
use {
    nix::sys::socket::{sockaddr_un, AddressFamily},
    std::ffi::OsStr,
    std::os::{raw::c_char, unix::ffi::OsStrExt},
};

#[cfg(windows)]
use winapi::shared::ws2def::{AF_INET, AF_INET6};

#[cfg(unix)]
const AF_INET: i32 = AddressFamily::Inet as i32;

#[cfg(unix)]
const AF_INET6: i32 = AddressFamily::Inet6 as i32;

#[cfg(unix)]
#[cfg_attr(feature = "cargo-clippy", allow(clippy::uninit_assumed_init))]
fn sun_path_offset() -> usize {
    use std::mem::MaybeUninit;

    let addr: sockaddr_un = unsafe { MaybeUninit::uninit().assume_init() };
    let base = &addr as *const _ as usize;
    let path = &addr.sun_path as *const _ as usize;

    path - base
}

#[cfg(unix)]
fn unix_socket_path(sockaddr: &SockAddr) -> String {
    let len = sockaddr.len() as usize - sun_path_offset();
    let raw_addr = unsafe { &*(sockaddr.as_ptr() as *mut sockaddr_un) };
    let path =
        unsafe { &*(&raw_addr.sun_path as *const [c_char] as *const [u8]) };

    if len == 0 || (cfg!(not(target_os = "linux")) && raw_addr.sun_path[0] == 0)
    {
        return String::new();
    }

    let (start, stop) = if raw_addr.sun_path[0] == 0 {
        (1, len)
    } else {
        (0, len - 1)
    };

    // Abstract names might contain NULL bytes and invalid UTF8. Since Inko
    // doesn't provide any better types at the moment we'll use a string and
    // convert the data to UTF8. A byte array would technically be better, but
    // these are mutable and make for an unpleasant runtime API.
    OsStr::from_bytes(&path[start..stop])
        .to_string_lossy()
        .into_owned()
}

#[cfg(not(unix))]
fn unix_socket_path(_sockaddr: &SockAddr) -> String {
    String::new()
}

/// A wrapper around the system's structure for socket addresses, such as
/// `sockaddr_un` for UNIX sockets.
pub enum SocketAddress {
    /// A UNIX socket.
    ///
    /// We use a separate enum variant because datagram UNIX sockets will have
    /// the family field set to AF_UNSPEC when using certain functions such as
    /// recvfrom().
    Unix(SockAddr),

    /// A socket of another type, such as an IPv4 or IPv6 socket.
    Other(SockAddr),
}

impl SocketAddress {
    pub fn address(&self) -> Result<(String, u64), String> {
        match self {
            SocketAddress::Unix(sockaddr) => {
                Ok((unix_socket_path(sockaddr), 0))
            }
            SocketAddress::Other(sockaddr) => {
                match i32::from(sockaddr.family()) {
                    AF_INET => {
                        let addr = sockaddr.as_inet().unwrap();

                        Ok((addr.ip().to_string(), u64::from(addr.port())))
                    }
                    AF_INET6 => {
                        let addr = sockaddr.as_inet6().unwrap();

                        Ok((addr.ip().to_string(), u64::from(addr.port())))
                    }
                    _ => Err(format!(
                        "The address family {} is not supported",
                        sockaddr.family()
                    )),
                }
            }
        }
    }
}
