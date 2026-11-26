// Windows x64 Unicode process environment helpers via Win32 kernel32.dll (*W APIs).
// All public APIs accept and return strict UTF-8; UTF-8 <-> UTF-16LE conversion occurs
// uniformly at the Win32 system boundary (GetEnvironmentVariableW, GetCurrentDirectoryW,
// GetCommandLineW).
import std.io
import std.store
import std.utf8

extern "C" fn GetEnvironmentVariableW(name: &u8, buf: &mut u8, size: u32) -> u32
contract { provides(noescape(name), noescape(buf)); effects(ffi); };
extern "C" fn GetCurrentDirectoryW(size: u32, buf: &mut u8) -> u32
contract { provides(noescape(buf)); effects(ffi); };
extern "C" fn GetCommandLineW() -> c_handle;
extern "C" fn GetLastError() -> u32;
extern "C" fn wcslen(s: c_handle) -> usize;
extern "C" fn memcpy(dst: &mut u8, src: c_handle, n: usize) -> c_handle
contract { provides(noescape(dst)); effects(ffi); };
extern "C" fn _exit(code: i32);

fn push_u16_le(buf: ByteBuffer, unit: i32) -> io.Error!void {
    if !bytes_push(buf, u8(unit % 256)) { return io.Error.InvalidResult }
    if !bytes_push(buf, u8(unit / 256)) { return io.Error.InvalidResult }
    return ()
}

fn utf8_to_utf16_nul(src: &[u8]) -> io.Error!ByteBuffer contract { result(create) } {
    if src.len() == 0 || src.len() > 4611686018427387900 { return io.Error.InvalidPath }
    let encoded = bytes_new()
    if !bytes_reserve(encoded, (src.len() + 1) * 2) { return io.Error.Allocation }
    var at: i64 = 0
    while at < src.len() {
        let s = utf8.decode(src, at)
        if s.status != 1 || s.value == 0 { return io.Error.InvalidPath }
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

fn read_u16_le(view: &[u8], unit_idx: i64) -> i32 {
    let off = unit_idx * 2
    return i32(view[off]) + i32(view[off + 1]) * 256
}

fn utf16le_to_utf8(wbytes: &[u8], units: i64) -> io.Error!ByteBuffer contract { result(create) } {
    if units < 0 || units * 2 > wbytes.len() { return io.Error.InvalidResult }
    let out = bytes_new()
    if units > 0 {
        if !bytes_reserve(out, units * 3) { return io.Error.Allocation }
    }
    var enc: [u8; 4] = [0u8, 0u8, 0u8, 0u8]
    var i: i64 = 0
    while i < units {
        let u1 = read_u16_le(wbytes, i)
        var scalar: i32 = u1
        if u1 >= 55296 && u1 <= 56319 {
            if i + 1 >= units { return io.Error.InvalidResult }
            let u2 = read_u16_le(wbytes, i + 1)
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
            if !bytes_push(out, enc[k]) { return io.Error.InvalidResult }
            k += 1
        }
    }
    return give out
}

fn alloc_zero_bytes(n: i64) -> io.Error!ByteBuffer contract { result(create) } {
    let buf = bytes_new()
    if n > 0 {
        if !bytes_reserve(buf, n) { return io.Error.Allocation }
        var i: i64 = 0
        while i < n {
            if !bytes_push(buf, 0u8) { return io.Error.InvalidResult }
            i += 1
        }
    }
    return give buf
}

fn query_env_into(wname: &[u8], dst: ByteBuffer, cap_units: u32) -> u32 {
    let view = bytes_view_mut(dst)
    var written: u32 = 0u32
    unsafe { written = GetEnvironmentVariableW(&wname[0], &mut view[0], cap_units) }
    return written
}

fn get_env_utf8(name: &[u8]) -> io.Error!Option<Managed<ByteBuffer>> {
    let wname = utf8_to_utf16_nul(name)?
    var probe: [u8; 2] = [0u8, 0u8]
    var needed: u32 = 0u32
    var err: u32 = 0u32
    unsafe {
        needed = GetEnvironmentVariableW(&bytes_view(wname)[0], &mut probe[0], 0u32)
        if needed == 0u32 { err = GetLastError() }
    }
    if needed == 0u32 {
        // ERROR_ENVVAR_NOT_FOUND = 203
        if err == 203u32 { return Option::<Managed<ByteBuffer>>.None() }
        if err == 0u32 {
            let empty = bytes_new()
            return Option::<Managed<ByteBuffer>>.Some(give empty)
        }
        return io.Error.Read
    }
    var cap = needed
    var attempts: i32 = 0
    while attempts < 8 {
        let cap_bytes = i64(cap) * 2
        let wbuf = alloc_zero_bytes(cap_bytes)?
        let written = query_env_into(bytes_view(wname), wbuf, cap)
        if written == 0u32 {
            var err2: u32 = 0u32
            unsafe { err2 = GetLastError() }
            if err2 == 203u32 { return Option::<Managed<ByteBuffer>>.None() }
            if err2 == 0u32 {
                let empty = bytes_new()
                return Option::<Managed<ByteBuffer>>.Some(give empty)
            }
            return io.Error.Read
        }
        if written < cap {
            let decoded = utf16le_to_utf8(bytes_view(wbuf), i64(written))?
            return Option::<Managed<ByteBuffer>>.Some(give decoded)
        }
        cap = written
        attempts += 1
    }
    return io.Error.Read
}

// Looks up an environment variable by UTF-8 name via Win32 GetEnvironmentVariableW,
// returning an owned UTF-8 ByteBuffer copy if present, or Option.None() if unset,
// empty name, containing embedded NUL, or invalid UTF-8.
pub fn get(name: &[u8]) -> Option<Managed<ByteBuffer>> {
    match get_env_utf8(name) {
        .err(_) => { return Option::<Managed<ByteBuffer>>.None() },
        .ok(opt) => { return opt }
    }
}

fn query_cwd_into(dst: ByteBuffer, cap_units: u32) -> u32 {
    let view = bytes_view_mut(dst)
    var written: u32 = 0u32
    unsafe { written = GetCurrentDirectoryW(cap_units, &mut view[0]) }
    return written
}

// Returns the current working directory as a UTF-8 ByteBuffer via Win32 GetCurrentDirectoryW.
pub fn current_dir() -> io.Error!ByteBuffer contract { result(create) } {
    var probe: [u8; 2] = [0u8, 0u8]
    var needed: u32 = 0u32
    unsafe { needed = GetCurrentDirectoryW(0u32, &mut probe[0]) }
    if needed == 0u32 { return io.Error.Read }
    var cap = needed
    var attempts: i32 = 0
    while attempts < 8 {
        let wbuf = alloc_zero_bytes(i64(cap) * 2)?
        let written = query_cwd_into(wbuf, cap)
        if written == 0u32 { return io.Error.Read }
        if written < cap {
            return utf16le_to_utf8(bytes_view(wbuf), i64(written))
        }
        cap = written
        attempts += 1
    }
    return io.Error.Read
}

fn copy_wide_from_c(dst: ByteBuffer, src: c_handle, n_bytes: usize) {
    let view = bytes_view_mut(dst)
    unsafe { memcpy(&mut view[0], src, n_bytes) }
}

fn get_cmdline_utf8() -> io.Error!ByteBuffer contract { result(create) } {
    var ptr = c_null()
    unsafe { ptr = GetCommandLineW() }
    if ptr == c_null() {
        let empty = bytes_new()
        return give empty
    }
    var units_u: usize = 0usize
    unsafe { units_u = wcslen(ptr) }
    if u64(units_u) > 4611686018427387900u64 { return io.Error.Capacity }
    let units = i64(units_u)
    if units == 0 {
        let empty = bytes_new()
        return give empty
    }
    let byte_len = units * 2
    let wbuf = alloc_zero_bytes(byte_len)?
    copy_wide_from_c(wbuf, ptr, usize(byte_len))
    return utf16le_to_utf8(bytes_view(wbuf), units)
}

fn is_cmd_ws(b: u8) -> bool {
    return b == 32u8 || b == 9u8
}

// Parses a Windows UTF-8 command line (decoded losslessly from GetCommandLineW)
// using Microsoft's documented CommandLineToArgvW / MSVC CRT argv parsing rules.
fn parse_win32_cmdline(cmd: &[u8]) -> io.Error!Store<Managed<ByteBuffer>> contract { result(create) } {
    let items: Store<Managed<ByteBuffer>> = store_new()
    var at: i64 = 0
    while at < cmd.len() && is_cmd_ws(cmd[at]) { at += 1 }
    if at >= cmd.len() { return give items }

    // 1. argv[0]: quoted string (no backslash escaping) or unquoted token up to space/tab.
    let arg0 = bytes_new()
    if cmd[at] == 34u8 {
        at += 1
        while at < cmd.len() && cmd[at] != 34u8 {
            if !bytes_push(arg0, cmd[at]) { return io.Error.Allocation }
            at += 1
        }
        if at < cmd.len() && cmd[at] == 34u8 { at += 1 }
    } else {
        while at < cmd.len() && !is_cmd_ws(cmd[at]) {
            if !bytes_push(arg0, cmd[at]) { return io.Error.Allocation }
            at += 1
        }
    }
    match store.push_growing(items, give arg0) {
        .ok(_) => {},
        .err(_) => { return io.Error.Allocation }
    }

    // 2. argv[1..]: standard Win32 backslash-before-quote + quote-toggle + "" rules.
    while at < cmd.len() {
        while at < cmd.len() && is_cmd_ws(cmd[at]) { at += 1 }
        if at >= cmd.len() { break }

        let cur = bytes_new()
        var in_quotes = false
        while at < cmd.len() {
            if !in_quotes && is_cmd_ws(cmd[at]) { break }
            var bs: i64 = 0
            while at < cmd.len() && cmd[at] == 92u8 {
                bs += 1
                at += 1
            }
            if at < cmd.len() && cmd[at] == 34u8 {
                var half: i64 = 0
                while half < bs / 2 {
                    if !bytes_push(cur, 92u8) { return io.Error.Allocation }
                    half += 1
                }
                if bs % 2 == 1 {
                    if !bytes_push(cur, 34u8) { return io.Error.Allocation }
                    at += 1
                } else {
                    if in_quotes && at + 1 < cmd.len() && cmd[at + 1] == 34u8 {
                        if !bytes_push(cur, 34u8) { return io.Error.Allocation }
                        at += 2
                    } else {
                        in_quotes = !in_quotes
                        at += 1
                    }
                }
            } else {
                var k: i64 = 0
                while k < bs {
                    if !bytes_push(cur, 92u8) { return io.Error.Allocation }
                    k += 1
                }
                if at < cmd.len() && (in_quotes || !is_cmd_ws(cmd[at])) {
                    if !bytes_push(cur, cmd[at]) { return io.Error.Allocation }
                    at += 1
                }
            }
        }
        match store.push_growing(items, give cur) {
            .ok(_) => {},
            .err(_) => { return io.Error.Allocation }
        }
    }
    return give items
}

// Collects all process command-line arguments (argv[0..argc]) as lossless UTF-8
// decoded from Win32 GetCommandLineW().
pub fn args() -> io.Error!Store<Managed<ByteBuffer>> contract { result(create) } {
    let cmd = get_cmdline_utf8()?
    return parse_win32_cmdline(bytes_view(cmd))
}

// Returns the number of process command-line arguments (argc, including argv[0]).
pub fn arg_count() -> i64 {
    match args() {
        .err(_) => { return 0 },
        .ok(items) => { return store_len(items) }
    }
}

// Returns an owned UTF-8 ByteBuffer copy of argv[index], or Option.None() if out of range.
pub fn arg(index: i64) -> Option<Managed<ByteBuffer>> {
    if index < 0 { return Option::<Managed<ByteBuffer>>.None() }
    match args() {
        .err(_) => { return Option::<Managed<ByteBuffer>>.None() },
        .ok(items) => {
            if index >= store_len(items) { return Option::<Managed<ByteBuffer>>.None() }
            let src = bytes_view(*store_at(items, index))
            let out = bytes_new()
            if src.len() > 0 {
                if !bytes_reserve(out, src.len()) { return Option::<Managed<ByteBuffer>>.None() }
                var i: i64 = 0
                while i < src.len() {
                    if !bytes_push(out, src[i]) { return Option::<Managed<ByteBuffer>>.None() }
                    i += 1
                }
            }
            return Option::<Managed<ByteBuffer>>.Some(give out)
        }
    }
}

// Terminates the current process immediately with the given exit code.
pub fn exit(code: i32) {
    unsafe { _exit(code) }
}
