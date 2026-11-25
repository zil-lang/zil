// Deterministic text normalization, redaction, word counting, and checksum helpers.
import std.path
import std.string

pub enum TransformError {
    InvalidUtf8,
    Capacity,
    Allocation
}

pub struct FileStats {
    pub lines: i64;
    pub words: i64;
    pub redacted: i64;
    pub output_bytes: i64;
    pub checksum: u64
}

pub package TransformedFile {
    manages content: ByteBuffer;
    pub stats: FileStats
}

pub fn content_bytes(file: &TransformedFile) -> &[u8] {
    return bytes_view(file.content)
}

pub fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false }
    var i: i64 = 0
    while i < a.len() {
        if a[i] != b[i] { return false }
        i += 1
    }
    return true
}

pub fn bytes_cmp(a: &[u8], b: &[u8]) -> i32 {
    var i: i64 = 0
    while i < a.len() && i < b.len() {
        if a[i] < b[i] { return -1 }
        if a[i] > b[i] { return 1 }
        i += 1
    }
    if a.len() < b.len() { return -1 }
    if a.len() > b.len() { return 1 }
    return 0
}

pub fn is_target_file(name: &[u8]) -> bool {
    if name.len() == 0 || name[0] == 46u8 {
        return false
    }
    let ext = path.extension(name)
    return bytes_eq(ext, "txt") || bytes_eq(ext, "log")
}

fn is_space(b: u8) -> bool {
    return b == 32u8 || b == 9u8 || b == 13u8 || b == 10u8
}

fn count_words(line: &[u8]) -> i64 {
    var count: i64 = 0
    var in_word = false
    var i: i64 = 0
    while i < line.len() {
        if is_space(line[i]) {
            in_word = false
        } else if !in_word {
            count += 1
            in_word = true
        }
        i += 1
    }
    return count
}

fn count_occurrences(text: &string.String, needle: &[u8]) -> i64 {
    var count: i64 = 0
    var pos: i64 = 0
    let total_scalars = string.scalar_len(text)
    while pos <= total_scalars {
        match string.find_from(text, needle, pos) {
            .err(_) => { break },
            .ok(found) => {
                if found < 0 { break }
                count += 1
                pos = found + needle.len()
            }
        }
    }
    return count
}

fn append_bytes(buf: ByteBuffer, data: &[u8]) -> TransformError!void {
    let cur = bytes_len(buf)
    let needed = cur + data.len()
    if needed > bytes_capacity(buf) {
        var next = bytes_capacity(buf)
        if next < 64 { next = 64 }
        while next < needed {
            next *= 2
        }
        if !bytes_reserve(buf, next) { return TransformError.Allocation }
    }
    var i: i64 = 0
    while i < data.len() {
        if !bytes_push(buf, data[i]) { return TransformError.Allocation }
        i += 1
    }
    return ()
}

fn uppercase_ascii(src: &[u8]) -> TransformError!ByteBuffer contract { result(create) } {
    let out = bytes_new()
    if !bytes_reserve(out, src.len()) { return TransformError.Allocation }
    var i: i64 = 0
    while i < src.len() {
        var b = src[i]
        if b >= 97u8 && b <= 122u8 {
            b = b - 32u8
        }
        if !bytes_push(out, b) { return TransformError.Allocation }
        i += 1
    }
    return give out
}

fn compute_checksum(data: &[u8]) -> u64 {
    var hash: u64 = 17u64
    var i: i64 = 0
    while i < data.len() {
        hash = (hash * 131u64 + u64(data[i]) + 1u64) % 1000000007u64
        i += 1
    }
    return hash
}

pub fn process_text(raw: &[u8]) -> TransformError!TransformedFile {
    let byte_limit: i64 = 1048576
    var doc = string.empty()
    match string.from_utf8(raw, byte_limit) {
        .err(e) => {
            match e {
                string.Error.InvalidUtf8() => { return TransformError.InvalidUtf8 },
                string.Error.Allocation() => { return TransformError.Allocation },
                _ => { return TransformError.Capacity }
            }
        },
        .ok(s) => { doc = s }
    }

    match string.lines(&doc, byte_limit) {
        .err(_) => { return TransformError.Capacity },
        .ok(line_store) => {
            let out = bytes_new()
            var kept_lines: i64 = 0
            var total_words: i64 = 0
            var total_redacted: i64 = 0
            var idx: i64 = 0
            while idx < store_len(line_store) {
                let raw_line = store_at(line_store, idx)
                match string.trim(raw_line, byte_limit) {
                    .err(_) => { return TransformError.Capacity },
                    .ok(trimmed) => {
                        if !string.is_empty(&trimmed) {
                            let upper_buf = uppercase_ascii(string.as_bytes(&trimmed))?
                            match string.from_utf8(bytes_view(upper_buf), byte_limit) {
                                .err(_) => { return TransformError.InvalidUtf8 },
                                .ok(upper_str) => {
                                    let secret_hits = count_occurrences(&upper_str, "SECRET")
                                    total_redacted += secret_hits
                                    match string.replace_all(&upper_str, "SECRET", "[REDACTED]", byte_limit) {
                                        .err(_) => { return TransformError.Capacity },
                                        .ok(redacted_str) => {
                                            let line_bytes = string.as_bytes(&redacted_str)
                                            total_words += count_words(line_bytes)
                                            append_bytes(out, line_bytes)?
                                            append_bytes(out, "\n")?
                                            kept_lines += 1
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                idx += 1
            }
            let out_view = bytes_view(out)
            let stats = FileStats {
                lines: kept_lines,
                words: total_words,
                redacted: total_redacted,
                output_bytes: out_view.len(),
                checksum: compute_checksum(out_view)
            }
            return TransformedFile {
                content: give out,
                stats: stats
            }
        }
    }
}
