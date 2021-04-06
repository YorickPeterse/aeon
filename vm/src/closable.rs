//! Files and sockets that can be closed manually.
//!
//! Rust IO (files, sockets, etc) structures can only be closed by dropping
//! them. This makes it difficult to allow Inko code to manually close files, as
//! doing so would require dropping the entire object. Using such objects can
//! then lead to cryptic errors when trying to use the object as a
//! file/socket/etc.
//!
//! To solve this, we introduce ClosableFile and ClosableSocket. These types
//! wrap a File and a socket2 Socket respectively, and allow manual closing of
//! these resources. These types also take care of handling double closing.
//!
//! The way we handle double closes is simple: when a file descriptor is closed,
//! it's swapped with an invalid one. This ensures that future closes don't end
//! up closing a recycled file descriptor.
use socket2::Socket;
use std::fs::File;
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};

#[cfg(unix)]
use {
    nix::errno::Errno,
    nix::unistd::close as close_raw,
    std::os::unix::io::{AsRawFd, FromRawFd, RawFd},
};

#[cfg(unix)]
const INVALID_FD: i32 = -2;

#[cfg(windows)]
use {
    std::os::windows::io::{
        AsRawHandle, AsRawSocket, FromRawHandle, FromRawSocket, RawHandle,
        RawSocket,
    },
    winapi::um::errhandlingapi::SetLastError,
    winapi::um::handleapi::{CloseHandle as close_raw, INVALID_HANDLE_VALUE},
    winapi::um::winnt::HANDLE,
    winapi::um::winsock2::{closesocket, INVALID_SOCKET, SOCKET},
};

#[cfg(unix)]
fn close_fd<F: AsRawFd>(file: &F) -> Option<RawFd> {
    let fd = file.as_raw_fd();

    if (fd as i32) == INVALID_FD {
        return None;
    }

    let _ = close_raw(fd);

    Errno::clear();
    Some(INVALID_FD as RawFd)
}

#[cfg(unix)]
fn close_file<F: AsRawFd>(file: &F) -> Option<File> {
    close_fd(file).map(|fd| unsafe { File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn close_socket<F: AsRawFd>(socket: &F) -> Option<Socket> {
    close_fd(socket).map(|fd| unsafe { Socket::from_raw_fd(fd) })
}

#[cfg(windows)]
fn close_file<F: AsRawHandle>(file: &F) -> Option<File> {
    let fd = file.as_raw_handle() as HANDLE;
    let invalid = INVALID_HANDLE_VALUE;

    if fd == invalid {
        return None;
    }

    unsafe {
        close_raw(fd);
        SetLastError(0);

        Some(File::from_raw_handle(invalid as RawHandle))
    }
}

#[cfg(windows)]
fn close_socket<F: AsRawSocket>(socket: &F) -> Option<Socket> {
    let fd = socket.as_raw_socket() as SOCKET;
    let invalid = INVALID_SOCKET;

    if fd == invalid {
        return None;
    }

    unsafe {
        closesocket(fd);
        SetLastError(0);

        Some(Socket::from_raw_socket(invalid as RawSocket))
    }
}

/// A file that can be closed manually.
pub struct ClosableFile {
    /// The File is wrapped in a ManuallyDrop so we have full control over how
    /// its resources are released.
    inner: ManuallyDrop<File>,
}

impl ClosableFile {
    pub fn new(file: File) -> Self {
        ClosableFile {
            inner: ManuallyDrop::new(file),
        }
    }

    pub fn close(&mut self) {
        if let Some(file) = close_file(&*self.inner) {
            // Using `*self.inner = X` drops the old value, so instead we use
            // this pattern.
            self.inner = ManuallyDrop::new(file);
        }
    }

    pub fn try_clone(&self) -> Result<Self, String> {
        Ok(Self::new(
            self.inner.try_clone().map_err(|e| e.to_string())?,
        ))
    }
}

impl Deref for ClosableFile {
    type Target = File;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ClosableFile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Drop for ClosableFile {
    fn drop(&mut self) {
        self.close();
    }
}

/// A socket that can be closed manually.
pub struct ClosableSocket {
    /// The Socket is wrapped in a ManuallyDrop so we have full control over how
    /// its resources are released.
    inner: ManuallyDrop<Socket>,
}

impl ClosableSocket {
    pub fn new(sock: Socket) -> Self {
        ClosableSocket {
            inner: ManuallyDrop::new(sock),
        }
    }

    pub fn close(&mut self) {
        if let Some(sock) = close_socket(&*self.inner) {
            // Using `*self.inner = X` drops the old value, so instead we use
            // this pattern.
            self.inner = ManuallyDrop::new(sock);
        }
    }
}

impl Deref for ClosableSocket {
    type Target = Socket;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ClosableSocket {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Drop for ClosableSocket {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use socket2::{Domain, Type};
    use std::io::Read;
    use std::path::PathBuf;

    #[test]
    fn test_close_file() {
        let readme = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("Cargo.toml")
            .to_string_lossy()
            .to_string();

        let mut file = ClosableFile::new(File::open(readme).unwrap());
        let mut buff = [0];

        assert!(file.read_exact(&mut buff).is_ok());

        file.close();

        assert!(file.read_exact(&mut buff).is_err());

        #[cfg(unix)]
        assert!((file.inner.as_raw_fd() as i32) == INVALID_FD);

        #[cfg(windows)]
        assert!((file.inner.as_raw_handle() as HANDLE) == INVALID_HANDLE_VALUE);
    }

    #[test]
    fn test_close_socket() {
        let mut socket = ClosableSocket::new(
            Socket::new(Domain::ipv4(), Type::dgram(), None).unwrap(),
        );

        assert!(socket.reuse_address().is_ok());

        socket.close();

        assert!(socket.reuse_address().is_err());

        #[cfg(unix)]
        assert!((socket.inner.as_raw_fd() as i32) == INVALID_FD);

        #[cfg(windows)]
        assert!((socket.inner.as_raw_socket() as SOCKET) == INVALID_SOCKET);
    }
}
