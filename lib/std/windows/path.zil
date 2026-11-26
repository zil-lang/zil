// Windows-aware byte path manipulation helpers.
// Accepts both '/' and '\' separators as well as drive prefixes (C:\, C:/)
// and UNC prefixes (\\server\share), while emitting '/' separators on join/normalize
// (universally accepted by Win32/UCRT APIs and matching cross-platform path output).
import std.io
import std.store

struct Segment { start: i64; end: i64 }

fn is_sep(b: u8) -> bool {
    return b == 47u8 || b == 92u8
}

fn is_ascii_alpha(b: u8) -> bool {
    return (b >= 65u8 && b <= 90u8) || (b >= 97u8 && b <= 122u8)
}

fn has_drive_prefix(path: &[u8]) -> bool {
    return path.len() >= 2 && is_ascii_alpha(path[0]) && path[1] == 58u8
}

fn is_unc_prefix(path: &[u8]) -> bool {
    return path.len() >= 2 && is_sep(path[0]) && is_sep(path[1])
}

fn is_absolute(path: &[u8]) -> bool {
    if path.len() == 0 { return false }
    if is_sep(path[0]) { return true }
    if has_drive_prefix(path) { return true }
    return false
}

fn copy_bytes(src: &[u8]) -> io.Error!ByteBuffer contract { result(create) } {
    let out = bytes_new()
    if !bytes_reserve(out, src.len()) { return io.Error.Allocation }
    var at: i64 = 0
    while at < src.len() {
        if !bytes_push(out, src[at]) { return io.Error.InvalidResult }
        at += 1
    }
    return give out
}

fn seg_is_dotdot(path: &[u8], seg: Segment) -> bool {
    return seg.end - seg.start == 2 && path[seg.start] == 46u8 && path[seg.start + 1] == 46u8
}

// Joins base and child with a single '/' separator.
// An absolute child replaces base; empty inputs return the other side.
pub fn join(base: &[u8], child: &[u8]) -> io.Error!ByteBuffer contract { result(create) } {
    if is_absolute(child) { return copy_bytes(child) }
    if base.len() == 0 { return copy_bytes(child) }
    if child.len() == 0 { return copy_bytes(base) }
    let last = base[base.len() - 1]
    let need_sep = !is_sep(last) && !(base.len() == 2 && has_drive_prefix(base))
    var sep_len: i64 = 0
    if need_sep { sep_len = 1 }
    if base.len() > 9223372036854775807 - sep_len { return io.Error.Capacity }
    let prefix_len = base.len() + sep_len
    if prefix_len > 9223372036854775807 - child.len() { return io.Error.Capacity }
    let total = prefix_len + child.len()
    let out = bytes_new()
    if !bytes_reserve(out, total) { return io.Error.Allocation }
    var at: i64 = 0
    while at < base.len() {
        if !bytes_push(out, base[at]) { return io.Error.InvalidResult }
        at += 1
    }
    if need_sep {
        if !bytes_push(out, 47u8) { return io.Error.InvalidResult }
    }
    at = 0
    while at < child.len() {
        if !bytes_push(out, child[at]) { return io.Error.InvalidResult }
        at += 1
    }
    return give out
}

// Returns the last non-empty path component after stripping trailing '/' and '\',
// or an empty slice for "/", "\", "C:\", "C:", or "".
pub fn file_name(path: &[u8]) -> &[u8] {
    var min_start: i64 = 0
    if has_drive_prefix(path) { min_start = 2 }
    var end = path.len()
    while end > min_start && is_sep(path[end - 1]) { end -= 1 }
    if end <= min_start { return &path[0..0] }
    var start = end
    while start > min_start && !is_sep(path[start - 1]) { start -= 1 }
    return &path[start..end]
}

// Returns the parent directory slice: "/" for "/foo", "C:\" for "C:\foo", "" for "foo", "/", or "C:\".
pub fn parent(path: &[u8]) -> &[u8] {
    var min_start: i64 = 0
    if has_drive_prefix(path) { min_start = 2 }
    var end = path.len()
    while end > min_start && is_sep(path[end - 1]) { end -= 1 }
    if end <= min_start { return &path[0..0] }
    var cut = end
    while cut > min_start && !is_sep(path[cut - 1]) { cut -= 1 }
    if cut <= min_start {
        if min_start > 0 {
            return &path[0..min_start]
        }
        return &path[0..0]
    }
    var parent_end = cut
    while parent_end > min_start + 1 && is_sep(path[parent_end - 1]) { parent_end -= 1 }
    return &path[0..parent_end]
}

// Returns the extension after the last '.' in file_name, excluding leading dot files
// such as ".gitignore".
pub fn extension(path: &[u8]) -> &[u8] {
    let name = file_name(path)
    if name.len() <= 1 { return &name[0..0] }
    var at = name.len() - 1
    while at > 0 {
        if name[at] == 46u8 {
            return &name[at + 1..name.len()]
        }
        at -= 1
    }
    return &name[0..0]
}

// Collapses repeated '/' and '\', resolves '.' segments, and resolves safe '..'
// segments (clamping at root/drive-root for absolute paths and preserving leading '..'
// for relative paths).
pub fn normalize(path: &[u8]) -> io.Error!ByteBuffer contract { result(create) } {
    if path.len() == 0 {
        let empty = bytes_new()
        return give empty
    }
    let has_drive = has_drive_prefix(path)
    let is_unc = !has_drive && is_unc_prefix(path)
    var root_abs = false
    var at: i64 = 0
    if has_drive {
        at = 2
        if at < path.len() && is_sep(path[at]) {
            root_abs = true
        }
    } else if is_unc {
        root_abs = true
        at = 2
    } else if is_sep(path[0]) {
        root_abs = true
    }

    let stack: Store<Segment> = store_new()
    while at < path.len() {
        while at < path.len() && is_sep(path[at]) { at += 1 }
        if at >= path.len() { break }
        let start = at
        while at < path.len() && !is_sep(path[at]) { at += 1 }
        let seg = Segment { start: start, end: at }
        let seg_len = at - start
        if seg_len == 1 && path[start] == 46u8 {
            // Skip "." segment.
        } else if seg_len == 2 && path[start] == 46u8 && path[start + 1] == 46u8 {
            let depth = store_len(stack)
            if depth > 0 {
                let top = *store_at(stack, depth - 1)
                if seg_is_dotdot(path, top) {
                    match store.push_growing(stack, seg) {
                        .err(_) => { return io.Error.Allocation },
                        .ok(_) => {}
                    }
                } else {
                    store_pop(stack)
                }
            } else if !root_abs {
                match store.push_growing(stack, seg) {
                    .err(_) => { return io.Error.Allocation },
                    .ok(_) => {}
                }
            }
        } else {
            match store.push_growing(stack, seg) {
                .err(_) => { return io.Error.Allocation },
                .ok(_) => {}
            }
        }
    }

    let count = store_len(stack)
    if count == 0 {
        let out = bytes_new()
        if has_drive {
            var cap: i64 = 2
            if root_abs { cap = 3 }
            if !bytes_reserve(out, cap) { return io.Error.Allocation }
            if !bytes_push(out, path[0]) { return io.Error.InvalidResult }
            if !bytes_push(out, 58u8) { return io.Error.InvalidResult }
            if root_abs {
                if !bytes_push(out, 47u8) { return io.Error.InvalidResult }
            }
            return give out
        }
        if is_unc {
            if !bytes_reserve(out, 2) { return io.Error.Allocation }
            if !bytes_push(out, 92u8) { return io.Error.InvalidResult }
            if !bytes_push(out, 92u8) { return io.Error.InvalidResult }
            return give out
        }
        if !bytes_reserve(out, 1) { return io.Error.Allocation }
        var ch: u8 = 46u8
        if root_abs { ch = 47u8 }
        if !bytes_push(out, ch) { return io.Error.InvalidResult }
        return give out
    }

    var total: i64 = 0
    if has_drive {
        total += 2
        if root_abs { total += 1 }
    } else if is_unc {
        total += 2
    } else if root_abs {
        total += 1
    }
    var idx: i64 = 0
    while idx < count {
        if idx > 0 {
            if total == 9223372036854775807 { return io.Error.Capacity }
            total += 1
        }
        let seg = *store_at(stack, idx)
        let len = seg.end - seg.start
        if total > 9223372036854775807 - len { return io.Error.Capacity }
        total += len
        idx += 1
    }

    let out = bytes_new()
    if !bytes_reserve(out, total) { return io.Error.Allocation }
    if has_drive {
        if !bytes_push(out, path[0]) { return io.Error.InvalidResult }
        if !bytes_push(out, 58u8) { return io.Error.InvalidResult }
        if root_abs {
            if !bytes_push(out, 47u8) { return io.Error.InvalidResult }
        }
    } else if is_unc {
        if !bytes_push(out, 92u8) { return io.Error.InvalidResult }
        if !bytes_push(out, 92u8) { return io.Error.InvalidResult }
    } else if root_abs {
        if !bytes_push(out, 47u8) { return io.Error.InvalidResult }
    }
    idx = 0
    while idx < count {
        if idx > 0 {
            if !bytes_push(out, 47u8) { return io.Error.InvalidResult }
        }
        let seg = *store_at(stack, idx)
        var pos = seg.start
        while pos < seg.end {
            if !bytes_push(out, path[pos]) { return io.Error.InvalidResult }
            pos += 1
        }
        idx += 1
    }
    return give out
}
