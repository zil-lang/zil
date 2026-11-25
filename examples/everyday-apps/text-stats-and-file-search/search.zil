// Substring line search across UTF-8 text content.
import std.io
import std.string

pub enum SearchError {
    InvalidQuery,
    InvalidUtf8,
    Capacity,
    Io,
}

fn map_print(s: &[u8]) -> SearchError!void {
    match io.print(s) {
        .ok(_) => { return () },
        .err(_) => { return SearchError.Io }
    }
}

fn map_println(s: &[u8]) -> SearchError!void {
    match io.println(s) {
        .ok(_) => { return () },
        .err(_) => { return SearchError.Io }
    }
}

fn map_print_i64(n: i64) -> SearchError!void {
    match io.print_i64(n) {
        .ok(_) => { return () },
        .err(_) => { return SearchError.Io }
    }
}

fn split_lines(content: &string.String, byte_limit: i64) -> SearchError!Store<string.String>
    contract { result(create) }
{
    match string.lines(content, byte_limit) {
        .ok(v) => { return give v },
        .err(_) => { return SearchError.Capacity }
    }
}

fn line_contains(line: &string.String, query: &[u8]) -> SearchError!bool {
    match string.contains(line, query) {
        .ok(v) => { return v },
        .err(_) => { return SearchError.InvalidUtf8 }
    }
}

fn line_trim(line: &string.String, byte_limit: i64) -> SearchError!string.String {
    match string.trim(line, byte_limit) {
        .ok(v) => { return v },
        .err(_) => { return SearchError.Capacity }
    }
}

pub fn search_content(
    display_path: &[u8],
    content: &string.String,
    query: &[u8],
    byte_limit: i64,
) -> SearchError!i64 {
    if query.len() == 0 {
        return SearchError.InvalidQuery
    }
    let lines = split_lines(content, byte_limit)?
    var matches: i64 = 0
    var idx: i64 = 0
    let total_lines = store_len(lines)
    while idx < total_lines {
        let line = store_at(lines, idx)
        let found = line_contains(line, query)?
        if found {
            let trimmed = line_trim(line, byte_limit)?
            map_print("MATCH ")?
            map_print(display_path)?
            map_print(":")?
            map_print_i64(idx + 1)?
            map_print(":")?
            map_println(string.as_bytes(&trimmed))?
            matches += 1
        }
        idx += 1
    }
    return matches
}
