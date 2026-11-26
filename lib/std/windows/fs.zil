// Windows x64 filesystem helpers via Win32 kernel32.dll and UCRT, built on std.io.
import std.io
import std.store

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

extern "C" fn _access(path: &u8, mode: i32) -> i32
contract { provides(noescape(path)); effects(ffi); };
extern "C" fn _unlink(path: &u8) -> i32
contract { provides(noescape(path)); effects(ffi); };
extern "C" fn GetFileAttributesExA(path: &u8, info_level: i32, info: &mut Win32FileAttrData) -> i32
contract { provides(noescape(path), noescape(info)); effects(ffi); };
extern "C" fn _mkdir(path: &u8) -> i32
contract { provides(noescape(path)); effects(ffi); };
extern "C" fn _rmdir(path: &u8) -> i32
contract { provides(noescape(path)); effects(ffi); };
extern "C" fn MoveFileExA(from: &u8, to: &u8, flags: u32) -> i32
contract { provides(noescape(from), noescape(to)); effects(ffi); };
extern "C" fn FindFirstFileA(pattern: &u8, find_data: &mut u8) -> i64
contract { provides(noescape(pattern), noescape(find_data)); effects(ffi); };
extern "C" fn FindNextFileA(handle: i64, find_data: &mut u8) -> i32
contract { provides(noescape(find_data)); effects(ffi); };
extern "C" fn FindClose(handle: i64) -> i32;
extern "C" fn GetLastError() -> u32;
extern "C" fn zil_glibc_errno_exchange(next: i32) -> i32;

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

fn c_path(path: &[u8]) -> RemoveError!ByteBuffer contract { result(create) } {
    if path.len() == 0 || path.len() == 9223372036854775807 { return RemoveError.InvalidPath }
    var at: i64 = 0
    while at < path.len() {
        if path[at] == 0u8 { return RemoveError.InvalidPath }
        at += 1
    }
    let encoded = bytes_new()
    if !bytes_reserve(encoded, path.len() + 1) { return RemoveError.Allocation }
    at = 0
    while at < path.len() {
        if !bytes_push(encoded, path[at]) { return RemoveError.InvalidResult }
        at += 1
    }
    if !bytes_push(encoded, 0u8) { return RemoveError.InvalidResult }
    return give encoded
}

fn io_c_path(path: &[u8]) -> io.Error!ByteBuffer contract { result(create) } {
    if path.len() == 0 || path.len() == 9223372036854775807 { return io.Error.InvalidPath }
    var at: i64 = 0
    while at < path.len() {
        if path[at] == 0u8 { return io.Error.InvalidPath }
        at += 1
    }
    let encoded = bytes_new()
    if !bytes_reserve(encoded, path.len() + 1) { return io.Error.Allocation }
    at = 0
    while at < path.len() {
        if !bytes_push(encoded, path[at]) { return io.Error.InvalidResult }
        at += 1
    }
    if !bytes_push(encoded, 0u8) { return io.Error.InvalidResult }
    return give encoded
}

fn make_search_pattern(path: &[u8]) -> io.Error!ByteBuffer contract { result(create) } {
    if path.len() == 0 || path.len() > 9223372036854775804 { return io.Error.InvalidPath }
    var at: i64 = 0
    while at < path.len() {
        if path[at] == 0u8 { return io.Error.InvalidPath }
        at += 1
    }
    let need_sep = !is_sep(path[path.len() - 1])
    var extra: i64 = 2
    if need_sep { extra = 3 }
    let out = bytes_new()
    if !bytes_reserve(out, path.len() + extra) { return io.Error.Allocation }
    at = 0
    while at < path.len() {
        if !bytes_push(out, path[at]) { return io.Error.InvalidResult }
        at += 1
    }
    if need_sep {
        if !bytes_push(out, 92u8) { return io.Error.InvalidResult }
    }
    // '*' = 42u8
    if !bytes_push(out, 42u8) { return io.Error.InvalidResult }
    if !bytes_push(out, 0u8) { return io.Error.InvalidResult }
    return give out
}

fn alloc_find_buf() -> io.Error!ByteBuffer contract { result(create) } {
    let buf = bytes_new()
    if !bytes_reserve(buf, 320) { return io.Error.Allocation }
    var i: i64 = 0
    while i < 320 {
        if !bytes_push(buf, 0u8) { return io.Error.InvalidResult }
        i += 1
    }
    return give buf
}

fn find_first_into(pattern: &[u8], buf: ByteBuffer) -> i64 {
    let dst = bytes_view_mut(buf)
    var h: i64 = -1
    unsafe { h = FindFirstFileA(&pattern[0], &mut dst[0]) }
    return h
}

fn find_next_into(handle: i64, buf: ByteBuffer) -> i32 {
    let dst = bytes_view_mut(buf)
    var ok: i32 = 0
    unsafe { ok = FindNextFileA(handle, &mut dst[0]) }
    return ok
}

// Extracts cFileName from WIN32_FIND_DATAA (starts at byte offset 44, max 260 bytes).
fn extract_find_name(buf: ByteBuffer) -> io.Error!Option<Managed<ByteBuffer>> {
    let view = bytes_view(buf)
    var nul: i64 = 44
    while nul < 304 && view[nul] != 0u8 { nul += 1 }
    if nul >= 304 { return io.Error.InvalidResult }
    let name_len = nul - 44
    if name_len == 1 && view[44] == 46u8 { return Option::<Managed<ByteBuffer>>.None() }
    if name_len == 2 && view[44] == 46u8 && view[45] == 46u8 {
        return Option::<Managed<ByteBuffer>>.None()
    }
    let item = bytes_new()
    if !bytes_reserve(item, name_len) { return io.Error.Allocation }
    var at: i64 = 0
    while at < name_len {
        if !bytes_push(item, view[44 + at]) { return io.Error.InvalidResult }
        at += 1
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

// Returns true iff path exists (F_OK = 0); returns false on empty path or embedded NUL.
pub fn exists(path: &[u8]) -> bool {
    match c_path(path) {
        .err(_) => { return false },
        .ok(name) => {
            var status: i32 = -1
            unsafe { status = _access(&bytes_view(name)[0], 0) }
            return status == 0
        }
    }
}

// Removes a file via UCRT _unlink.
pub fn remove_file(path: &[u8]) -> RemoveError!void {
    let name = c_path(path)?
    var status: i32 = -1
    unsafe { status = _unlink(&bytes_view(name)[0]) }
    if status != 0 { return RemoveError.Remove }
    return ()
}

// Queries Win32 GetFileAttributesExA metadata.
pub fn metadata(path: &[u8]) -> io.Error!Metadata {
    let name = io_c_path(path)?
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
    unsafe { ok = GetFileAttributesExA(&bytes_view(name)[0], 0, &mut info) }
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

// Creates a single directory via UCRT _mkdir.
pub fn mkdir(path: &[u8]) -> io.Error!void {
    let name = io_c_path(path)?
    var status: i32 = -1
    unsafe { status = _mkdir(&bytes_view(name)[0]) }
    if status != 0 { return io.Error.Write }
    return ()
}

// Creates a directory and all missing parent components iteratively.
// Succeeds if the target directory already exists; fails if a component is a non-directory.
pub fn mkdir_all(path: &[u8]) -> io.Error!void {
    if path.len() == 0 || path.len() == 9223372036854775807 { return io.Error.InvalidPath }
    var check: i64 = 0
    while check < path.len() {
        if path[check] == 0u8 { return io.Error.InvalidPath }
        check += 1
    }
    var start: i64 = 1
    // Skip Windows drive prefix "C:" / "C:\" so we never call _mkdir("C:").
    if path.len() >= 2 && is_ascii_alpha(path[0]) && path[1] == 58u8 {
        start = 3
    }
    var at = start
    while at <= path.len() {
        if at == path.len() || is_sep(path[at]) {
            if at > 0 && !is_sep(path[at - 1]) {
                let prefix = &path[0..at]
                if !is_dir(prefix) {
                    let name = io_c_path(prefix)?
                    var status: i32 = -1
                    var err: i32 = 0
                    unsafe {
                        zil_glibc_errno_exchange(0)
                        status = _mkdir(&bytes_view(name)[0])
                        err = zil_glibc_errno_exchange(0)
                    }
                    if status != 0 {
                        // EEXIST = 17 on Windows UCRT.
                        if !(err == 17 && is_dir(prefix)) { return io.Error.Write }
                    }
                }
            }
        }
        at += 1
    }
    if !is_dir(path) { return io.Error.Write }
    return ()
}

// Removes an empty directory via UCRT _rmdir.
pub fn remove_dir(path: &[u8]) -> io.Error!void {
    let name = io_c_path(path)?
    var status: i32 = -1
    unsafe { status = _rmdir(&bytes_view(name)[0]) }
    if status != 0 { return io.Error.Write }
    return ()
}

// Renames a file or directory, replacing an existing destination (MOVEFILE_REPLACE_EXISTING = 1).
pub fn rename(from: &[u8], to: &[u8]) -> io.Error!void {
    let c_from = io_c_path(from)?
    let c_to = io_c_path(to)?
    var ok: i32 = 0
    unsafe { ok = MoveFileExA(&bytes_view(c_from)[0], &bytes_view(c_to)[0], 1u32) }
    if ok == 0 { return io.Error.Write }
    return ()
}

// Reads directory entry names (skipping "." and "..") into an owned Store.
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
