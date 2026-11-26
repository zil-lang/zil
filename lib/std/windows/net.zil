// Windows x64 Winsock2 TCP networking (IPv4 TcpListener and TcpStream).
// Each TcpStream and TcpListener owns at most one SOCKET handle (u64/i64;
// INVALID_SOCKET = -1) and deterministically closes it via closesocket on
// explicit close() or Drop.
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

extern "C" fn WSAStartup(version_requested: u16, wsa_data: &mut u8) -> i32
contract { provides(noescape(wsa_data)); effects(ffi); };
extern "C" fn WSAGetLastError() -> i32;
extern "C" fn socket(domain: i32, sock_type: i32, protocol: i32) -> i64;
extern "C" fn setsockopt(sock: i64, level: i32, optname: i32, optval: &i32, optlen: i32) -> i32
contract { provides(noescape(optval)); effects(ffi); };
extern "C" fn bind(sock: i64, addr: &SockAddrIn, addrlen: i32) -> i32
contract { provides(noescape(addr)); effects(ffi); };
extern "C" fn listen(sock: i64, backlog: i32) -> i32;
extern "C" fn accept(sock: i64, addr: c_handle, addrlen: c_handle) -> i64;
extern "C" fn connect(sock: i64, addr: &SockAddrIn, addrlen: i32) -> i32
contract { provides(noescape(addr)); effects(ffi); };
extern "C" fn getsockname(sock: i64, addr: &mut SockAddrIn, addrlen: &mut i32) -> i32
contract { provides(noescape(addr), noescape(addrlen)); effects(ffi); };
extern "C" fn recv(sock: i64, buf: &mut u8, len: i32, flags: i32) -> i32
contract { provides(noescape(buf)); effects(ffi); };
extern "C" fn send(sock: i64, buf: &u8, len: i32, flags: i32) -> i32
contract { provides(noescape(buf)); effects(ffi); };
extern "C" fn shutdown(sock: i64, how: i32) -> i32;
extern "C" fn closesocket(sock: i64) -> i32;

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

fn wsa_init_into(buf: ByteBuffer) -> i32 {
    let dst = bytes_view_mut(buf)
    var rc: i32 = -1
    // MAKEWORD(2, 2) = 0x0202 (514)
    unsafe { rc = WSAStartup(514u16, &mut dst[0]) }
    return rc
}

fn ensure_winsock() -> io.Error!void {
    // WSADATA on Windows x64 is 408 bytes; allocate a 416-byte scratch buffer.
    let buf = bytes_new()
    if !bytes_reserve(buf, 416) { return io.Error.Allocation }
    var i: i64 = 0
    while i < 416 {
        if !bytes_push(buf, 0u8) { return io.Error.InvalidResult }
        i += 1
    }
    let rc = wsa_init_into(buf)
    if rc != 0 { return io.Error.Open }
    return ()
}

fn make_sockaddr_in(addr: Ipv4Addr, port: i32) -> SockAddrIn {
    // AF_INET = 2 on Windows; port is big-endian network byte order.
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
    sock: i64
}

impl Drop for TcpStream {
    fn drop(self: Self) {
        if .sock != -1 {
            unsafe { closesocket(.sock) }
        }
    }
}

fn read_sock(sock: i64, buf: &mut [u8]) -> io.Error!i64 {
    if sock == -1 {
        return io.Error.Read
    }
    let len = buf.len()
    if len == 0 {
        return 0
    }
    var req = len
    if req > 1073741824 {
        req = 1073741824
    }
    var count: i32 = 0
    loop {
        unsafe {
            count = recv(sock, &mut buf[0], i32(req), 0)
        }
        if count < 0 {
            var err: i32 = 0
            unsafe { err = WSAGetLastError() }
            // WSAEINTR = 10004
            if err == 10004 {
                continue
            }
            return io.Error.Read
        }
        break
    }
    let c64 = i64(count)
    if c64 > req {
        return io.Error.InvalidResult
    }
    return c64
}

fn write_sock(sock: i64, data: &[u8]) -> io.Error!i64 {
    if sock == -1 {
        return io.Error.Write
    }
    let len = data.len()
    if len == 0 {
        return 0
    }
    var req = len
    if req > 1073741824 {
        req = 1073741824
    }
    var written: i32 = 0
    loop {
        unsafe {
            written = send(sock, &data[0], i32(req), 0)
        }
        if written < 0 {
            var err: i32 = 0
            unsafe { err = WSAGetLastError() }
            // WSAEINTR = 10004
            if err == 10004 {
                continue
            }
            return io.Error.Write
        }
        break
    }
    if written == 0 {
        return io.Error.NoProgress
    }
    let w64 = i64(written)
    if w64 > req {
        return io.Error.InvalidResult
    }
    return w64
}

impl TcpStream {
    pub fn connect(addr: Ipv4Addr, port: i32) -> io.Error!TcpStream contract { result(create) } {
        if port < 0 || port > 65535 {
            return io.Error.InvalidPath
        }
        ensure_winsock()?
        var sock: i64 = -1
        // AF_INET = 2, SOCK_STREAM = 1, protocol = 0
        unsafe { sock = socket(2, 1, 0) }
        if sock == -1 {
            return io.Error.Open
        }
        let sa = make_sockaddr_in(addr, port)
        var status: i32 = -1
        unsafe { status = connect(sock, &sa, 16) }
        if status != 0 {
            unsafe { closesocket(sock) }
            return io.Error.Open
        }
        return TcpStream { sock: sock }
    }

    pub fn close(self: &mut TcpStream) -> io.Error!void {
        if self.sock == -1 {
            return ()
        }
        let sock = self.sock
        self.sock = -1
        var status: i32 = -1
        unsafe { status = closesocket(sock) }
        if status != 0 {
            return io.Error.Write
        }
        return ()
    }

    pub fn read(self: &mut TcpStream, buf: &mut [u8]) -> io.Error!i64 {
        return read_sock(self.sock, buf)
    }

    pub fn read_to_end(self: &mut TcpStream, byte_limit: i64) -> io.Error!ByteBuffer contract { result(create) } {
        let sock = self.sock
        if sock == -1 {
            return io.Error.Read
        }
        if byte_limit < 0 {
            return io.Error.Capacity
        }
        let data = bytes_new()
        var chunk = empty_chunk()
        loop {
            let count = read_sock(sock, &mut chunk[0..256])?
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
        return write_sock(self.sock, data)
    }

    pub fn write_all(self: &mut TcpStream, data: &[u8]) -> io.Error!void {
        let sock = self.sock
        if sock == -1 {
            return io.Error.Write
        }
        let total = data.len()
        var at: i64 = 0
        while at < total {
            let written = write_sock(sock, &data[at..total])?
            at += written
        }
        return ()
    }

    pub fn shutdown_write(self: &mut TcpStream) -> io.Error!void {
        if self.sock == -1 {
            return io.Error.Write
        }
        var status: i32 = -1
        // SD_SEND = 1 on Winsock2
        unsafe { status = shutdown(self.sock, 1) }
        if status != 0 {
            return io.Error.Write
        }
        return ()
    }
}

pub resource TcpListener {
    sock: i64
}

impl Drop for TcpListener {
    fn drop(self: Self) {
        if .sock != -1 {
            unsafe { closesocket(.sock) }
        }
    }
}

impl TcpListener {
    pub fn bind(addr: Ipv4Addr, port: i32) -> io.Error!TcpListener contract { result(create) } {
        if port < 0 || port > 65535 {
            return io.Error.InvalidPath
        }
        ensure_winsock()?
        var sock: i64 = -1
        // AF_INET = 2, SOCK_STREAM = 1, protocol = 0
        unsafe { sock = socket(2, 1, 0) }
        if sock == -1 {
            return io.Error.Open
        }
        let reuse: i32 = 1
        var opt_status: i32 = -1
        // SOL_SOCKET = 0xFFFF (65535), SO_REUSEADDR = 4 on Winsock2
        unsafe { opt_status = setsockopt(sock, 65535, 4, &reuse, 4) }
        if opt_status != 0 {
            unsafe { closesocket(sock) }
            return io.Error.Open
        }
        let sa = make_sockaddr_in(addr, port)
        var bind_status: i32 = -1
        unsafe { bind_status = bind(sock, &sa, 16) }
        if bind_status != 0 {
            unsafe { closesocket(sock) }
            return io.Error.Open
        }
        var listen_status: i32 = -1
        unsafe { listen_status = listen(sock, 128) }
        if listen_status != 0 {
            unsafe { closesocket(sock) }
            return io.Error.Open
        }
        return TcpListener { sock: sock }
    }

    pub fn local_port(self: &TcpListener) -> io.Error!i32 {
        if self.sock == -1 {
            return io.Error.Read
        }
        var sa = zero_sockaddr_in()
        var len: i32 = 16
        var status: i32 = -1
        unsafe { status = getsockname(self.sock, &mut sa, &mut len) }
        if status != 0 || len < 4 {
            return io.Error.Read
        }
        return i32(sa.sin_port_hi) * 256 + i32(sa.sin_port_lo)
    }

    pub fn accept(self: &TcpListener) -> io.Error!TcpStream contract { result(create) } {
        if self.sock == -1 {
            return io.Error.Open
        }
        var client_sock: i64 = -1
        loop {
            unsafe {
                client_sock = accept(self.sock, c_null(), c_null())
            }
            if client_sock == -1 {
                var err: i32 = 0
                unsafe { err = WSAGetLastError() }
                // WSAEINTR = 10004
                if err == 10004 {
                    continue
                }
                return io.Error.Open
            }
            break
        }
        return TcpStream { sock: client_sock }
    }

    pub fn close(self: &mut TcpListener) -> io.Error!void {
        if self.sock == -1 {
            return ()
        }
        let sock = self.sock
        self.sock = -1
        var status: i32 = -1
        unsafe { status = closesocket(sock) }
        if status != 0 {
            return io.Error.Write
        }
        return ()
    }
}
