// Linux x86_64 POSIX TCP networking (IPv4 TcpListener and TcpStream).
// All foreign calls are synchronous, do not retain buffer addresses, reenter
// ZIL, or unwind. Each TcpStream and TcpListener owns at most one socket fd
// and deterministically closes it on explicit close() or Drop.
import std.io

struct SockAddrIn {
    sin_family: u16;
    sin_port_hi: u8;
    sin_port_lo: u8;
    addr0: u8;
    addr1: u8;
    addr2: u8;
    addr3: u8;
    sin_zero: u64
}

extern "C" fn socket(domain: i32, sock_type: i32, protocol: i32) -> i32;
extern "C" fn setsockopt(fd: i32, level: i32, optname: i32, optval: &i32, optlen: u32) -> i32
contract { provides(noescape(optval)); effects(ffi); };
extern "C" fn bind(fd: i32, addr: &SockAddrIn, addrlen: u32) -> i32
contract { provides(noescape(addr)); effects(ffi); };
extern "C" fn listen(fd: i32, backlog: i32) -> i32;
extern "C" fn accept(fd: i32, addr: c_handle, addrlen: c_handle) -> i32;
extern "C" fn connect(fd: i32, addr: &SockAddrIn, addrlen: u32) -> i32
contract { provides(noescape(addr)); effects(ffi); };
extern "C" fn getsockname(fd: i32, addr: &mut SockAddrIn, addrlen: &mut u32) -> i32
contract { provides(noescape(addr), noescape(addrlen)); effects(ffi); };
extern "C" fn recv(fd: i32, buf: &mut u8, len: usize, flags: i32) -> i64
contract { provides(noescape(buf)); effects(ffi); };
extern "C" fn send(fd: i32, buf: &u8, len: usize, flags: i32) -> i64
contract { provides(noescape(buf)); effects(ffi); };
extern "C" fn shutdown(fd: i32, how: i32) -> i32;
extern "C" fn close(fd: i32) -> i32;
extern "C" fn zil_glibc_errno_exchange(next: i32) -> i32;

pub struct Ipv4Addr {
    pub a: u8;
    pub b: u8;
    pub c: u8;
    pub d: u8
}

pub fn ipv4(a: u8, b: u8, c: u8, d: u8) -> Ipv4Addr {
    return Ipv4Addr { a: a, b: b, c: c, d: d }
}

pub fn localhost() -> Ipv4Addr {
    return Ipv4Addr { a: 127u8, b: 0u8, c: 0u8, d: 1u8 }
}

fn make_sockaddr_in(addr: Ipv4Addr, port: i32) -> SockAddrIn {
    // AF_INET = 2 on Linux x86_64; port is big-endian network byte order.
    let hi = u8(port / 256)
    let lo = u8(port % 256)
    return SockAddrIn {
        sin_family: 2u16,
        sin_port_hi: hi,
        sin_port_lo: lo,
        addr0: addr.a,
        addr1: addr.b,
        addr2: addr.c,
        addr3: addr.d,
        sin_zero: 0u64,
    }
}

fn zero_sockaddr_in() -> SockAddrIn {
    return SockAddrIn {
        sin_family: 0u16,
        sin_port_hi: 0u8,
        sin_port_lo: 0u8,
        addr0: 0u8,
        addr1: 0u8,
        addr2: 0u8,
        addr3: 0u8,
        sin_zero: 0u64,
    }
}

fn empty_chunk() -> [u8; 256] {
    return [
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
    ]
}

pub resource TcpStream {
    fd: i32
}

impl Drop for TcpStream {
    fn drop(self: Self) {
        if .fd >= 0 {
            unsafe { close(.fd) }
        }
    }
}

fn read_fd(fd: i32, buf: &mut [u8]) -> io.Error!i64 {
    if fd < 0 {
        return io.Error.Read
    }
    let len = buf.len()
    if len == 0 {
        return 0
    }
    var count: i64 = 0
    loop {
        unsafe {
            zil_glibc_errno_exchange(0)
            count = recv(fd, &mut buf[0], usize(len), 0)
        }
        if count < 0 {
            var err: i32 = 0
            unsafe { err = zil_glibc_errno_exchange(0) }
            // EINTR = 4 on Linux x86_64
            if err == 4 {
                continue
            }
            return io.Error.Read
        }
        break
    }
    if count > len {
        return io.Error.InvalidResult
    }
    return count
}

fn write_fd(fd: i32, data: &[u8]) -> io.Error!i64 {
    if fd < 0 {
        return io.Error.Write
    }
    let len = data.len()
    if len == 0 {
        return 0
    }
    var written: i64 = 0
    loop {
        unsafe {
            zil_glibc_errno_exchange(0)
            // MSG_NOSIGNAL = 0x4000 (16384) on Linux x86_64 prevents SIGPIPE.
            written = send(fd, &data[0], usize(len), 16384)
        }
        if written < 0 {
            var err: i32 = 0
            unsafe { err = zil_glibc_errno_exchange(0) }
            // EINTR = 4 on Linux x86_64
            if err == 4 {
                continue
            }
            return io.Error.Write
        }
        break
    }
    if written == 0 {
        return io.Error.NoProgress
    }
    if written > len {
        return io.Error.InvalidResult
    }
    return written
}

impl TcpStream {
    pub fn connect(addr: Ipv4Addr, port: i32) -> io.Error!TcpStream contract { result(create) } {
        if port < 0 || port > 65535 {
            return io.Error.InvalidPath
        }
        var fd: i32 = -1
        // AF_INET = 2, SOCK_STREAM = 1, protocol = 0
        unsafe { fd = socket(2, 1, 0) }
        if fd < 0 {
            return io.Error.Open
        }
        let sa = make_sockaddr_in(addr, port)
        var status: i32 = -1
        unsafe { status = connect(fd, &sa, 16u32) }
        if status < 0 {
            unsafe { close(fd) }
            return io.Error.Open
        }
        return TcpStream { fd: fd }
    }

    pub fn close(self: &mut TcpStream) -> io.Error!void {
        if self.fd < 0 {
            return ()
        }
        let fd = self.fd
        self.fd = -1
        var status: i32 = -1
        unsafe { status = close(fd) }
        if status < 0 {
            return io.Error.Write
        }
        return ()
    }

    pub fn read(self: &mut TcpStream, buf: &mut [u8]) -> io.Error!i64 {
        return read_fd(self.fd, buf)
    }

    pub fn read_to_end(self: &mut TcpStream, byte_limit: i64) -> io.Error!ByteBuffer contract { result(create) } {
        let fd = self.fd
        if fd < 0 {
            return io.Error.Read
        }
        if byte_limit < 0 {
            return io.Error.Capacity
        }
        let data = bytes_new()
        var chunk = empty_chunk()
        loop {
            let count = read_fd(fd, &mut chunk[0..256])?
            if count == 0 {
                break
            }
            let length = bytes_len(data)
            if length > byte_limit - count {
                return io.Error.Capacity
            }
            let needed = length + count
            if needed > bytes_capacity(data) {
                var next = bytes_capacity(data)
                if next < 256 {
                    next = 256
                }
                while next < needed {
                    if next > byte_limit / 2 {
                        next = byte_limit
                        break
                    }
                    next *= 2
                }
                if next > byte_limit {
                    next = byte_limit
                }
                if !bytes_reserve(data, next) {
                    return io.Error.Allocation
                }
            }
            var at: i64 = 0
            while at < count {
                if !bytes_push(data, chunk[at]) {
                    return io.Error.InvalidResult
                }
                at += 1
            }
        }
        return give data
    }

    pub fn write(self: &mut TcpStream, data: &[u8]) -> io.Error!i64 {
        return write_fd(self.fd, data)
    }

    pub fn write_all(self: &mut TcpStream, data: &[u8]) -> io.Error!void {
        let fd = self.fd
        if fd < 0 {
            return io.Error.Write
        }
        let total = data.len()
        var at: i64 = 0
        while at < total {
            let written = write_fd(fd, &data[at..total])?
            at += written
        }
        return ()
    }

    pub fn shutdown_write(self: &mut TcpStream) -> io.Error!void {
        if self.fd < 0 {
            return io.Error.Write
        }
        var status: i32 = -1
        // SHUT_WR = 1 on Linux x86_64
        unsafe { status = shutdown(self.fd, 1) }
        if status < 0 {
            return io.Error.Write
        }
        return ()
    }
}

pub resource TcpListener {
    fd: i32
}

impl Drop for TcpListener {
    fn drop(self: Self) {
        if .fd >= 0 {
            unsafe { close(.fd) }
        }
    }
}

impl TcpListener {
    pub fn bind(addr: Ipv4Addr, port: i32) -> io.Error!TcpListener contract { result(create) } {
        if port < 0 || port > 65535 {
            return io.Error.InvalidPath
        }
        var fd: i32 = -1
        // AF_INET = 2, SOCK_STREAM = 1, protocol = 0
        unsafe { fd = socket(2, 1, 0) }
        if fd < 0 {
            return io.Error.Open
        }
        let reuse: i32 = 1
        var opt_status: i32 = -1
        // SOL_SOCKET = 1, SO_REUSEADDR = 2 on Linux x86_64
        unsafe { opt_status = setsockopt(fd, 1, 2, &reuse, 4u32) }
        if opt_status < 0 {
            unsafe { close(fd) }
            return io.Error.Open
        }
        let sa = make_sockaddr_in(addr, port)
        var bind_status: i32 = -1
        unsafe { bind_status = bind(fd, &sa, 16u32) }
        if bind_status < 0 {
            unsafe { close(fd) }
            return io.Error.Open
        }
        var listen_status: i32 = -1
        unsafe { listen_status = listen(fd, 128) }
        if listen_status < 0 {
            unsafe { close(fd) }
            return io.Error.Open
        }
        return TcpListener { fd: fd }
    }

    pub fn local_port(self: &TcpListener) -> io.Error!i32 {
        if self.fd < 0 {
            return io.Error.Read
        }
        var sa = zero_sockaddr_in()
        var len: u32 = 16u32
        var status: i32 = -1
        unsafe { status = getsockname(self.fd, &mut sa, &mut len) }
        if status < 0 || len < 4u32 {
            return io.Error.Read
        }
        return i32(sa.sin_port_hi) * 256 + i32(sa.sin_port_lo)
    }

    pub fn accept(self: &TcpListener) -> io.Error!TcpStream contract { result(create) } {
        if self.fd < 0 {
            return io.Error.Open
        }
        var client_fd: i32 = -1
        loop {
            unsafe {
                zil_glibc_errno_exchange(0)
                client_fd = accept(self.fd, c_null(), c_null())
            }
            if client_fd < 0 {
                var err: i32 = 0
                unsafe { err = zil_glibc_errno_exchange(0) }
                // EINTR = 4 on Linux x86_64
                if err == 4 {
                    continue
                }
                return io.Error.Open
            }
            break
        }
        return TcpStream { fd: client_fd }
    }

    pub fn close(self: &mut TcpListener) -> io.Error!void {
        if self.fd < 0 {
            return ()
        }
        let fd = self.fd
        self.fd = -1
        var status: i32 = -1
        unsafe { status = close(fd) }
        if status < 0 {
            return io.Error.Write
        }
        return ()
    }
}
