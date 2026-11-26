// Windows x64 Unicode filesystem helpers via Win32 kernel32.dll (*W APIs), built on std.io.
// All public APIs accept and return strict UTF-8; UTF-8 <-> UTF-16LE conversion occurs
// uniformly at the Win32 system boundary.
import std.io
import std.store
import std.utf8

struct Win32FileAttrData {
    dw_file_attributes: u32;
    ft_creation_lo: u32;
    ft_creation_hi: u32;
    ft_access_lo: u32;
    ft_access_hi: u32;
    ft_write_lo: u32;
    ft_write_hi: u32;
    n_file_size_high: u32;
    n_file_size_low: u32
}

extern "C" fn GetFileAttributesW(path: &u8) -> u32
contract { provides(noescape(path)); effects(ffi); };
extern "C" fn DeleteFileW(path: &u8) -> i32
contract { provides(noescape(path)); effects(ffi); };
extern "C" fn GetFileAttributesExW(path: &u8, info_level: i32, info: &mut Win32FileAttrData) -> i32
contract { provides(noescape(path), noescape(info)); effects(ffi); };
extern "C" fn CreateDirectoryW(path: &u8, security_attrs: c_handle) -> i32
contract { provides(noescape(path)); effects(ffi); };
extern "C" fn RemoveDirectoryW(path: &u8) -> i32
contract { provides(noescape(path)); effects(ffi); };
extern "C" fn MoveFileExW(from: &u8, to: &u8, flags: u32) -> i32
contract { provides(noescape(from), noescape(to)); effects(ffi); };
extern "C" fn FindFirstFileW(pattern: &u8, find_data: &mut u8) -> i64
contract { provides(noescape(pattern), noescape(find_data)); effects(ffi); };
extern "C" fn FindNextFileW(handle: i64, find_data: &mut u8) -> i32
contract { provides(noescape(find_data)); effects(ffi); };
extern "C" fn FindClose(handle: i64) -> i32;
extern "C" fn GetLastError() -> u32;

pub enum RemoveError { InvalidPath, Allocation, InvalidResult, Remove }
pub struct Metadata { pub size: i64; pub is_dir: bool; pub is_file: bool }

resource FindHandle { h: i64 }
impl Drop for FindHandle { fn drop(self: Self) { if .h != -1 { unsafe { FindClose(.h) } } } }

fn is_sep(b: u8) -> bool {
    return b == 47u8 || b == 92u8
}

fn is_ascii_alpha(b: u8) -> bool {
    return (b >= 65u8 && b <= 90u8) || (b >= 97u8 && b <= 122u8)
}

fn push_u16_le_io(buf: ByteBuffer, unit: i32) -> io.Error!void {
    if !bytes_push(buf, u8(unit % 256)) { return io.Error.InvalidResult }
    if !bytes_push(buf, u8(unit / 256)) { return io.Error.InvalidResult }
    return ()
}

fn push_u16_le_rm(buf: ByteBuffer, unit: i32) -> RemoveError!void {
    if !bytes_push(buf, u8(unit % 256)) { return RemoveError.InvalidResult }
    if !bytes_push(buf, u8(unit / 256)) { return RemoveError.InvalidResult }
    return ()
}

fn remove_wpath(path: &[u8]) -> RemoveError!ByteBuffer contract { result(create) } {
    if path.len() == 0 || path.len() > 4611686018427387900 { return RemoveError.InvalidPath }
    let encoded = bytes_new()
    if !bytes_reserve(encoded, (path.len() + 1) * 2) { return RemoveError.Allocation }
    var at: i64 = 0
    while at < path.len() {
        let s = utf8.decode(path, at)
        if s.status != 1 || s.value == 0 { return RemoveError.InvalidPath }
        if s.value < 65536 {
            push_u16_le_rm(encoded, s.value)?
        } else {
            let adj = s.value - 65536
            push_u16_le_rm(encoded, 55296 + (adj / 1024))?
            push_u16_le_rm(encoded, 56320 + (adj % 1024))?
        }
        at = s.next
    }
    push_u16_le_rm(encoded, 0)?
    return give encoded
}

fn io_wpath(path: &[u8]) -> io.Error!ByteBuffer contract { result(create) } {
    if path.len() == 0 || path.len() > 4611686018427387900 { return io.Error.InvalidPath }
    let encoded = bytes_new()
    if !bytes_reserve(encoded, (path.len() + 1) * 2) { return io.Error.Allocation }
    var at: i64 = 0
    while at < path.len() {
        let s = utf8.decode(path, at)
        if s.status != 1 || s.value == 0 { return io.Error.InvalidPath }
        if s.value < 65536 {
            push_u16_le_io(encoded, s.value)?
        } else {
            let adj = s.value - 65536
            push_u16_le_io(encoded, 55296 + (adj / 1024))?
            push_u16_le_io(encoded, 56320 + (adj % 1024))?
        }
        at = s.next
    }
    push_u16_le_io(encoded, 0)?
    return give encoded
}

fn make_search_pattern(path: &[u8]) -> io.Error!ByteBuffer contract { result(create) } {
    if path.len() == 0 || path.len() > 4611686018427387898 { return io.Error.InvalidPath }
    let need_sep = !is_sep(path[path.len() - 1])
    var extra: i64 = 2
    if need_sep { extra = 3 }
    let out = bytes_new()
    if !bytes_reserve(out, (path.len() + extra) * 2) { return io.Error.Allocation }
    var at: i64 = 0
    while at < path.len() {
        let s = utf8.decode(path, at)
        if s.status != 1 || s.value == 0 { return io.Error.InvalidPath }
        if s.value < 65536 {
            push_u16_le_io(out, s.value)?
        } else {
            let adj = s.value - 65536
            push_u16_le_io(out, 55296 + (adj / 1024))?
            push_u16_le_io(out, 56320 + (adj % 1024))?
        }
        at = s.next
    }
    if need_sep {
        push_u16_le_io(out, 92)?
    }
    // '*' = 42
    push_u16_le_io(out, 42)?
    push_u16_le_io(out, 0)?
    return give out
}

// WIN32_FIND_DATAW is 592 bytes on x64 (cFileName[260] WCHARs at byte offset 44..564).
fn alloc_find_buf() -> io.Error!ByteBuffer contract { result(create) } {
    let buf = bytes_new()
    if !bytes_reserve(buf, 608) { return io.Error.Allocation }
    var i: i64 = 0
    while i < 608 {
        if !bytes_push(buf, 0u8) { return io.Error.InvalidResult }
        i += 1
    }
    return give buf
}

fn find_first_into(pattern: &[u8], buf: ByteBuffer) -> i64 {
    let dst = bytes_view_mut(buf)
    var h: i64 = -1
    unsafe { h = FindFirstFileW(&pattern[0], &mut dst[0]) }
    return h
}

fn find_next_into(handle: i64, buf: ByteBuffer) -> i32 {
    let dst = bytes_view_mut(buf)
    var ok: i32 = 0
    unsafe { ok = FindNextFileW(handle, &mut dst[0]) }
    return ok
}

fn read_u16_le(view: &[u8], byte_offset: i64) -> i32 {
    return i32(view[byte_offset]) + i32(view[byte_offset + 1]) * 256
}

// Extracts cFileName from WIN32_FIND_DATAW (offset 44, max 260 WCHARs) and decodes
// UTF-16LE into guaranteed valid UTF-8, skipping "." and "..".
fn extract_find_name(buf: ByteBuffer) -> io.Error!Option<Managed<ByteBuffer>> {
    let view = bytes_view(buf)
    var units: i64 = 0
    while units < 260 && read_u16_le(view, 44 + units * 2) != 0 {
        units += 1
    }
    if units >= 260 { return io.Error.InvalidResult }
    if units == 1 && read_u16_le(view, 44) == 46 {
        return Option::<Managed<ByteBuffer>>.None()
    }
    if units == 2 && read_u16_le(view, 44) == 46 && read_u16_le(view, 46) == 46 {
        return Option::<Managed<ByteBuffer>>.None()
    }
    let item = bytes_new()
    if !bytes_reserve(item, units * 3) { return io.Error.Allocation }
    var enc: [u8; 4] = [0u8, 0u8, 0u8, 0u8]
    var i: i64 = 0
    while i < units {
        let u1 = read_u16_le(view, 44 + i * 2)
        var scalar: i32 = u1
        if u1 >= 55296 && u1 <= 56319 {
            if i + 1 >= units { return io.Error.InvalidResult }
            let u2 = read_u16_le(view, 44 + (i + 1) * 2)
            if u2 < 56320 || u2 > 57343 { return io.Error.InvalidResult }
            scalar = 65536 + (u1 - 55296) * 1024 + (u2 - 56320)
            i += 2
        } else if u1 >= 56320 && u1 <= 57343 {
            return io.Error.InvalidResult
        } else {
            i += 1
        }
        let width = utf8.encode(scalar, &mut enc[..])
        if width <= 0 { return io.Error.InvalidResult }
        var k: i64 = 0
        while k < width {
            if !bytes_push(item, enc[k]) { return io.Error.InvalidResult }
            k += 1
        }
    }
    return Option::<Managed<ByteBuffer>>.Some(give item)
}

// Opens path for reading, reads all bytes into an owned ByteBuffer, and closes
// the descriptor on both success and error paths via File's Drop.
pub fn read_to_bytes(path: &[u8]) -> io.Error!ByteBuffer contract { result(create) } {
    let file = io.open_read(path)?
    return io.read_to_end(&file, 9223372036854775807)
}

// Creates or truncates path, writes all bytes, and closes cleanly.
pub fn write_bytes(path: &[u8], data: &[u8]) -> io.Error!void {
    let file = io.create_write(path, 420)?
    return io.write_all(&file, data)
}

// Returns true iff path exists; returns false on empty path, embedded NUL, or invalid UTF-8.
pub fn exists(path: &[u8]) -> bool {
    match remove_wpath(path) {
        .err(_) => { return false },
        .ok(wpath) => {
            var attrs: u32 = 4294967295u32
            unsafe { attrs = GetFileAttributesW(&bytes_view(wpath)[0]) }
            // INVALID_FILE_ATTRIBUTES = 0xFFFFFFFF (4294967295)
            return attrs != 4294967295u32
        }
    }
}

// Removes a file via Win32 DeleteFileW.
pub fn remove_file(path: &[u8]) -> RemoveError!void {
    let wpath = remove_wpath(path)?
    var ok: i32 = 0
    unsafe { ok = DeleteFileW(&bytes_view(wpath)[0]) }
    if ok == 0 { return RemoveError.Remove }
    return ()
}

// Queries Win32 GetFileAttributesExW metadata.
pub fn metadata(path: &[u8]) -> io.Error!Metadata {
    let wpath = io_wpath(path)?
    var info = Win32FileAttrData {
        dw_file_attributes: 0u32,
        ft_creation_lo: 0u32,
        ft_creation_hi: 0u32,
        ft_access_lo: 0u32,
        ft_access_hi: 0u32,
        ft_write_lo: 0u32,
        ft_write_hi: 0u32,
        n_file_size_high: 0u32,
        n_file_size_low: 0u32,
    }
    var ok: i32 = 0
    unsafe { ok = GetFileAttributesExW(&bytes_view(wpath)[0], 0, &mut info) }
    if ok == 0 { return io.Error.Read }
    // FILE_ATTRIBUTE_DIRECTORY = 0x10 (16)
    let is_dir_flag = ((info.dw_file_attributes / 16u32) % 2u32) != 0u32
    let raw_size = u64(info.n_file_size_high) * 4294967296u64 + u64(info.n_file_size_low)
    if raw_size > 9223372036854775807u64 { return io.Error.InvalidResult }
    let size = i64(raw_size)
    return Metadata { size: size, is_dir: is_dir_flag, is_file: !is_dir_flag }
}

// Returns true iff path exists and refers to a directory.
pub fn is_dir(path: &[u8]) -> bool {
    match metadata(path) {
        .err(_) => { return false },
        .ok(meta) => { return meta.is_dir }
    }
}

// Returns true iff path exists and refers to a regular file.
pub fn is_file(path: &[u8]) -> bool {
    match metadata(path) {
        .err(_) => { return false },
        .ok(meta) => { return meta.is_file }
    }
}

// Returns the size in bytes reported by Win32 metadata for path.
pub fn file_size(path: &[u8]) -> io.Error!i64 {
    let meta = metadata(path)?
    return meta.size
}

// Creates a single directory via Win32 CreateDirectoryW.
pub fn mkdir(path: &[u8]) -> io.Error!void {
    let wpath = io_wpath(path)?
    var ok: i32 = 0
    unsafe { ok = CreateDirectoryW(&bytes_view(wpath)[0], c_null()) }
    if ok == 0 { return io.Error.Write }
    return ()
}

// Creates a directory and all missing parent components iteratively.
// Succeeds if the target directory already exists; fails if a component is a non-directory.
pub fn mkdir_all(path: &[u8]) -> io.Error!void {
    let _validated = io_wpath(path)?
    var start: i64 = 1
    // Skip Windows drive prefix "C:" / "C:\" so we never call CreateDirectoryW("C:").
    if path.len() >= 2 && is_ascii_alpha(path[0]) && path[1] == 58u8 {
        start = 3
    }
    var at = start
    while at <= path.len() {
        if at == path.len() || is_sep(path[at]) {
            if at > 0 && !is_sep(path[at - 1]) {
                let prefix = &path[0..at]
                if !is_dir(prefix) {
                    let wprefix = io_wpath(prefix)?
                    var ok: i32 = 0
                    var err: u32 = 0u32
                    unsafe {
                        ok = CreateDirectoryW(&bytes_view(wprefix)[0], c_null())
                        if ok == 0 { err = GetLastError() }
                    }
                    if ok == 0 {
                        // ERROR_ALREADY_EXISTS = 183
                        if !(err == 183u32 && is_dir(prefix)) { return io.Error.Write }
                    }
                }
            }
        }
        at += 1
    }
    if !is_dir(path) { return io.Error.Write }
    return ()
}

// Removes an empty directory via Win32 RemoveDirectoryW.
pub fn remove_dir(path: &[u8]) -> io.Error!void {
    let wpath = io_wpath(path)?
    var ok: i32 = 0
    unsafe { ok = RemoveDirectoryW(&bytes_view(wpath)[0]) }
    if ok == 0 { return io.Error.Write }
    return ()
}

// Renames a file or directory, replacing an existing destination (MOVEFILE_REPLACE_EXISTING = 1).
pub fn rename(from: &[u8], to: &[u8]) -> io.Error!void {
    let wfrom = io_wpath(from)?
    let wto = io_wpath(to)?
    var ok: i32 = 0
    unsafe { ok = MoveFileExW(&bytes_view(wfrom)[0], &bytes_view(wto)[0], 1u32) }
    if ok == 0 { return io.Error.Write }
    return ()
}

// Reads directory entry names as valid UTF-8 (skipping "." and "..") into an owned Store.
// The Win32 find handle is always closed by FindHandle's Drop on every exit path.
pub fn read_dir(path: &[u8]) -> io.Error!Store<Managed<ByteBuffer>> contract { result(create) } {
    let pattern = make_search_pattern(path)?
    if !is_dir(path) { return io.Error.Open }
    let find_buf = alloc_find_buf()?
    let raw_h = find_first_into(bytes_view(pattern), find_buf)
    let entries: Store<Managed<ByteBuffer>> = store_new()
    if raw_h == -1 {
        var err: u32 = 0u32
        unsafe { err = GetLastError() }
        // ERROR_FILE_NOT_FOUND = 2, ERROR_NO_MORE_FILES = 18
        if err == 2u32 || err == 18u32 {
            return give entries
        }
        return io.Error.Open
    }
    let dir = FindHandle { h: raw_h }
    loop {
        match extract_find_name(find_buf)? {
            Option.None() => {},
            Option.Some(item) => {
                match store.push_growing(entries, item) {
                    .err(_) => { return io.Error.Allocation },
                    .ok(_) => {}
                }
            }
        }
        let has_next = find_next_into(dir.h, find_buf)
        if has_next == 0 {
            var err: u32 = 0u32
            unsafe { err = GetLastError() }
            if err != 18u32 { return io.Error.Read }
            break
        }
    }
    return give entries
}
