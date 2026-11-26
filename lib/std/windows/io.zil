// Windows x64 UCRT binary byte I/O with strict UTF-8 -> UTF-16LE path conversion.
// Every opened descriptor uses _wsopen_s(..., _O_BINARY, _SH_DENYNO, ...) and
// standard streams switch to _O_BINARY so LF bytes are preserved verbatim without
// CRLF text-mode translation. A File owns exactly one fd.
import std.utf8

extern "C" fn _read(fd: i32, buffer: &mut [u8; 64], count: u32) -> i32
contract { provides(noescape(buffer)); effects(ffi); };
extern "C" fn _write(fd: i32, buffer: &[u8; 64], count: u32) -> i32
contract { provides(noescape(buffer)); effects(ffi); };
extern "C" fn _dup(fd: i32) -> i32;
extern "C" fn _close(fd: i32) -> i32;
extern "C" fn _setmode(fd: i32, mode: i32) -> i32;
extern "C" fn _wsopen_s(pfd: &mut i32, path: &u8, oflag: i32, shflag: i32, pmode: i32) -> i32
contract { provides(noescape(pfd), noescape(path)); effects(ffi); };
extern "C" fn zil_glibc_errno_exchange(next: i32) -> i32;

pub enum Error { Duplicate, Open, InvalidPath, Capacity, Allocation, Read, Write, NoProgress, InvalidResult }
pub struct ReadResult { pub count: i64; pub eof: bool }
pub resource File { fd: i32 }
impl Drop for File { fn drop(self: Self) { unsafe { _close(.fd) } } }

// The returned duplicate has independent ownership. Dropping it closes only
// that descriptor; the caller still owns the original fd.
pub fn duplicate(fd: i32) -> Error!File contract { result(create) } {
    var owned: i32 = -1
    unsafe { owned = _dup(fd) }
    if owned < 0 { return Error.Duplicate }
    // _O_BINARY = 0x8000 (32768)
    unsafe { let _ = _setmode(owned, 32768) }
    return File { fd: owned }
}
pub fn stdin() -> Error!File contract { result(create) } { return duplicate(0) }
pub fn stdout() -> Error!File contract { result(create) } { return duplicate(1) }
pub fn stderr() -> Error!File contract { result(create) } { return duplicate(2) }

fn push_u16_le(buf: ByteBuffer, unit: i32) -> Error!void {
    if !bytes_push(buf, u8(unit % 256)) { return Error.InvalidResult }
    if !bytes_push(buf, u8(unit / 256)) { return Error.InvalidResult }
    return ()
}

// Encodes a non-empty NUL-free UTF-8 path to NUL-terminated UTF-16LE bytes.
// ByteBuffer allocates via Win32 HeapAlloc (16-byte aligned >= alignof(wchar_t)).
fn utf8_path_to_utf16(path: &[u8]) -> Error!ByteBuffer contract { result(create) } {
    if path.len() == 0 || path.len() > 4611686018427387900 { return Error.InvalidPath }
    let encoded = bytes_new()
    if !bytes_reserve(encoded, (path.len() + 1) * 2) { return Error.Allocation }
    var at: i64 = 0
    while at < path.len() {
        let s = utf8.decode(path, at)
        if s.status != 1 || s.value == 0 { return Error.InvalidPath }
        if s.value < 65536 {
            push_u16_le(encoded, s.value)?
        } else {
            let adj = s.value - 65536
            push_u16_le(encoded, 55296 + (adj / 1024))?
            push_u16_le(encoded, 56320 + (adj % 1024))?
        }
        at = s.next
    }
    push_u16_le(encoded, 0)?
    return give encoded
}

// Opens a UTF-8 path read-only in binary mode via UCRT _wsopen_s(_O_RDONLY|_O_BINARY, _SH_DENYNO).
pub fn open_read(path: &[u8]) -> Error!File contract { result(create) } {
    let wpath = utf8_path_to_utf16(path)?
    var fd: i32 = -1
    var rc: i32 = -1
    // _O_RDONLY (0) | _O_BINARY (0x8000 = 32768), _SH_DENYNO = 0x40 (64)
    unsafe { rc = _wsopen_s(&mut fd, &bytes_view(wpath)[0], 32768, 64, 0) }
    if rc != 0 || fd < 0 { return Error.Open }
    return File { fd: fd }
}

// Creates or truncates a UTF-8 path write-only in binary mode via UCRT _wsopen_s.
pub fn create_write(path: &[u8], _mode: i32) -> Error!File contract { result(create) } {
    let wpath = utf8_path_to_utf16(path)?
    var fd: i32 = -1
    var rc: i32 = -1
    // _O_WRONLY (1) | _O_CREAT (0x0100) | _O_TRUNC (0x0200) | _O_BINARY (0x8000) = 33537
    // _SH_DENYNO = 0x40 (64), _S_IREAD (0x0100) | _S_IWRITE (0x0080) = 384
    unsafe { rc = _wsopen_s(&mut fd, &bytes_view(wpath)[0], 33537, 64, 384) }
    if rc != 0 || fd < 0 { return Error.Open }
    return File { fd: fd }
}

pub fn empty_buffer() -> [u8; 64] {
    return [0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
            0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
            0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
            0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
            0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
            0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
            0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
            0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8]
}

// A short read is a successful count, zero is EOF, and -1 is an I/O error.
// Interrupted calls (EINTR = 4) are retried automatically.
pub fn read_chunk(file: &File, buffer: &mut [u8; 64]) -> Error!ReadResult {
    var count: i32 = 0
    loop {
        unsafe {
            zil_glibc_errno_exchange(0)
            count = _read(file.fd, buffer, 64u32)
        }
        if count < 0 {
            var err: i32 = 0
            unsafe { err = zil_glibc_errno_exchange(0) }
            if err == 4 { continue }
            return Error.Read
        }
        break
    }
    if count > 64 { return Error.InvalidResult }
    return ReadResult { count: i64(count), eof: count == 0 }
}

// Read to EOF with an explicit byte limit.
pub fn read_to_end(file: &File, byte_limit: i64) -> Error!ByteBuffer contract { result(create) } {
    if byte_limit < 0 { return Error.Capacity }
    let data = bytes_new()
    var block = empty_buffer()
    loop {
        let result = read_chunk(file, &mut block)?
        if result.eof { break }
        let length = bytes_len(data)
        if length > byte_limit - result.count { return Error.Capacity }
        let needed = length + result.count
        if needed > bytes_capacity(data) {
            var next = bytes_capacity(data)
            if next < 64 { next = 64 }
            while next < needed {
                if next > byte_limit / 2 { next = byte_limit; break }
                next *= 2
            }
            if next > byte_limit { next = byte_limit }
            if !bytes_reserve(data, next) { return Error.Allocation }
        }
        var at: i64 = 0
        while at < result.count {
            if !bytes_push(data, block[at]) { return Error.InvalidResult }
            at += 1
        }
    }
    return give data
}

fn write_fd(fd: i32, source: &[u8]) -> Error!void {
    if fd == 1 || fd == 2 {
        // Ensure standard output/error preserve raw LF bytes on Windows.
        unsafe { let _ = _setmode(fd, 32768) }
    }
    var block = empty_buffer()
    var at: i64 = 0
    while at < source.len() {
        var length = source.len() - at
        if length > 64 { length = 64 }
        var copied: i64 = 0
        while copied < length { block[copied] = source[at + copied]; copied += 1 }
        var remaining = length
        while remaining > 0 {
            var written: i32 = 0
            unsafe {
                zil_glibc_errno_exchange(0)
                written = _write(fd, &block, u32(remaining))
            }
            if written < 0 {
                var err: i32 = 0
                unsafe { err = zil_glibc_errno_exchange(0) }
                if err == 4 { continue }
                return Error.Write
            }
            if written == 0 { return Error.NoProgress }
            let w64 = i64(written)
            if w64 > remaining { return Error.InvalidResult }
            remaining -= w64
            var index: i64 = 0
            while index < remaining { block[index] = block[index + w64]; index += 1 }
        }
        at += length
    }
    return ()
}

pub fn write_all(file: &File, source: &[u8]) -> Error!void {
    return write_fd(file.fd, source)
}

pub fn print(bytes: &[u8]) -> Error!void {
    return write_fd(1, bytes)
}

pub fn println(bytes: &[u8]) -> Error!void {
    write_fd(1, bytes)?
    return write_fd(1, "\n")
}

pub fn eprint(bytes: &[u8]) -> Error!void {
    return write_fd(2, bytes)
}

pub fn eprintln(bytes: &[u8]) -> Error!void {
    write_fd(2, bytes)?
    return write_fd(2, "\n")
}

pub fn print_i64(value: i64) -> Error!void {
    if value == 0 { return write_fd(1, "0") }
    var buf = empty_buffer()
    var pos: i64 = 64
    var cur = value
    let neg = cur < 0
    if !neg { cur = 0 - cur }
    while cur < 0 {
        let rem = cur % 10
        let digit: u8 = u8(48 - rem)
        pos -= 1
        buf[pos] = digit
        cur /= 10
    }
    if neg {
        pos -= 1
        buf[pos] = 45u8
    }
    return write_fd(1, &buf[pos..64])
}

pub fn println_i64(value: i64) -> Error!void {
    print_i64(value)?
    return write_fd(1, "\n")
}

pub fn write_chunk(file: &File, buffer: &[u8; 64], count: i64) -> Error!i64 {
    if count < 0 || count > 64 { return Error.Capacity }
    if count == 0 { return i64(0) }
    var written: i32 = 0
    loop {
        unsafe {
            zil_glibc_errno_exchange(0)
            written = _write(file.fd, buffer, u32(count))
        }
        if written < 0 {
            var err: i32 = 0
            unsafe { err = zil_glibc_errno_exchange(0) }
            if err == 4 { continue }
            return Error.Write
        }
        break
    }
    if written == 0 { return Error.NoProgress }
    let w64 = i64(written)
    if w64 > count { return Error.InvalidResult }
    return w64
}
