// Everyday CLI application: recursive text statistics and file search.
// Usage:
//   text-stats-and-file-search stats <path>
//   text-stats-and-file-search search <path> <query>
//   text-stats-and-file-search summary <path> <query>
import std.env
import std.fs
import std.io
import std.path
import std.store
import std.string
import stats
import search

enum AppError {
    BadArgs,
    MissingPath,
    EmptyQuery,
    InvalidUtf8,
    Allocation,
    Io,
}

fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false
    }
    var at: i64 = 0
    while at < a.len() {
        if a[at] != b[at] {
            return false
        }
        at += 1
    }
    return true
}

fn bytes_less(a: &[u8], b: &[u8]) -> bool {
    var at: i64 = 0
    while at < a.len() && at < b.len() {
        if a[at] < b[at] {
            return true
        }
        if a[at] > b[at] {
            return false
        }
        at += 1
    }
    return a.len() < b.len()
}

fn out_print(s: &[u8]) -> AppError!void {
    match io.print(s) {
        .ok(_) => { return () },
        .err(_) => { return AppError.Io }
    }
}

fn out_print_i64(n: i64) -> AppError!void {
    match io.print_i64(n) {
        .ok(_) => { return () },
        .err(_) => { return AppError.Io }
    }
}

fn out_println_i64(n: i64) -> AppError!void {
    match io.println_i64(n) {
        .ok(_) => { return () },
        .err(_) => { return AppError.Io }
    }
}

fn require_arg(index: i64) -> AppError!Managed<ByteBuffer> {
    match env.arg(index) {
        Option.Some(v) => { return v },
        Option.None() => { return AppError.BadArgs }
    }
}

fn remove_entry(names: Store<Managed<ByteBuffer>>, index: i64) -> AppError!Managed<ByteBuffer> {
    match store.remove::<Managed<ByteBuffer>>(names, index) {
        .ok(v) => { return v },
        .err(_) => { return AppError.Allocation }
    }
}

fn sort_names(give names: Store<Managed<ByteBuffer>>) -> AppError!Store<Managed<ByteBuffer>>
    contract { result(create) }
{
    let sorted: Store<Managed<ByteBuffer>> = store_new()
    let total = store_len(names)
    if total > 0 {
        if !store_reserve(sorted, total) {
            return AppError.Allocation
        }
    }
    while store_len(names) > 0 {
        var best: i64 = 0
        var idx: i64 = 1
        let remaining = store_len(names)
        while idx < remaining {
            let candidate = store_at(names, idx)
            let current = store_at(names, best)
            if bytes_less(bytes_view(candidate), bytes_view(current)) {
                best = idx
            }
            idx += 1
        }
        let chosen = remove_entry(names, best)?
        match store_push(sorted, chosen) {
            .ok(_) => {},
            .err(_) => { return AppError.Allocation }
        }
    }
    return give sorted
}

fn read_file_bytes(full_path: &[u8]) -> AppError!ByteBuffer
    contract { result(create) }
{
    match fs.read_to_bytes(full_path) {
        .ok(b) => { return give b },
        .err(_) => { return AppError.Io }
    }
}

fn read_dir_entries(full_path: &[u8]) -> AppError!Store<Managed<ByteBuffer>>
    contract { result(create) }
{
    match fs.read_dir(full_path) {
        .ok(items) => { return give items },
        .err(_) => { return AppError.Io }
    }
}

fn join_paths(base: &[u8], child: &[u8]) -> AppError!ByteBuffer
    contract { result(create) }
{
    match path.join(base, child) {
        .ok(p) => { return give p },
        .err(_) => { return AppError.Allocation }
    }
}

fn decode_utf8(raw: &[u8], byte_limit: i64) -> AppError!string.String {
    match string.from_utf8(raw, byte_limit) {
        .ok(s) => { return s },
        .err(_) => { return AppError.InvalidUtf8 }
    }
}

fn compute_stats(content: &string.String, byte_limit: i64) -> AppError!stats.FileStats {
    match stats.count_text(content, byte_limit) {
        .ok(s) => { return s },
        .err(_) => { return AppError.InvalidUtf8 }
    }
}

fn run_search(
    display_path: &[u8],
    content: &string.String,
    query: &[u8],
    byte_limit: i64,
) -> AppError!i64 {
    match search.search_content(display_path, content, query, byte_limit) {
        .ok(n) => { return n },
        .err(e) => {
            match e {
                search.SearchError.InvalidQuery() => { return AppError.EmptyQuery },
                search.SearchError.InvalidUtf8() => { return AppError.InvalidUtf8 },
                search.SearchError.Capacity() => { return AppError.Allocation },
                search.SearchError.Io() => { return AppError.Io },
            }
        }
    }
}

fn print_file_stats(display_path: &[u8], one: stats.FileStats) -> AppError!void {
    out_print("FILE ")?
    out_print(display_path)?
    out_print(" lines=")?
    out_print_i64(one.lines)?
    out_print(" words=")?
    out_print_i64(one.words)?
    out_print(" bytes=")?
    out_println_i64(one.bytes)?
    return ()
}

fn process_file(
    full_path: &[u8],
    display_path: &[u8],
    do_stats: bool,
    do_search: bool,
    query: &[u8],
    totals: &mut stats.TotalStats,
    total_matches: &mut i64,
) -> AppError!void {
    let raw = read_file_bytes(full_path)?
    let byte_limit: i64 = 16777216
    let content = decode_utf8(bytes_view(raw), byte_limit)?
    if do_stats {
        let one = compute_stats(&content, byte_limit)?
        stats.add_file_stats(totals, one)
        print_file_stats(display_path, one)?
    }
    if do_search {
        let matched = run_search(display_path, &content, query, byte_limit)?
        *total_matches += matched
    }
    return ()
}

fn walk_path(
    full_path: &[u8],
    display_path: &[u8],
    do_stats: bool,
    do_search: bool,
    query: &[u8],
    totals: &mut stats.TotalStats,
    total_matches: &mut i64,
    depth: i64,
) -> AppError!void {
    if depth > 32 {
        return AppError.Io
    }
    if fs.is_file(full_path) {
        return process_file(
            full_path,
            display_path,
            do_stats,
            do_search,
            query,
            totals,
            total_matches,
        )
    }
    if !fs.is_dir(full_path) {
        return AppError.MissingPath
    }
    let raw_entries = read_dir_entries(full_path)?
    let entries = sort_names(give raw_entries)?
    var idx: i64 = 0
    let count = store_len(entries)
    while idx < count {
        let name = store_at(entries, idx)
        let child_full = join_paths(full_path, bytes_view(name))?
        let child_display = join_paths(display_path, bytes_view(name))?
        walk_path(
            bytes_view(child_full),
            bytes_view(child_display),
            do_stats,
            do_search,
            query,
            totals,
            total_matches,
            depth + 1,
        )?
        idx += 1
    }
    return ()
}

fn run_cli() -> AppError!void {
    let argc = env.arg_count()
    if argc < 3 {
        return AppError.BadArgs
    }
    let mode_arg = require_arg(1)?
    let target_arg = require_arg(2)?
    let mode = bytes_view(mode_arg)
    let target = bytes_view(target_arg)

    let is_stats = bytes_eq(mode, "stats")
    let is_search = bytes_eq(mode, "search")
    let is_summary = bytes_eq(mode, "summary")
    if !is_stats && !is_search && !is_summary {
        return AppError.BadArgs
    }

    if is_stats {
        if argc != 3 {
            return AppError.BadArgs
        }
        if !fs.exists(target) {
            return AppError.MissingPath
        }
        var totals = stats.empty_totals()
        var matches: i64 = 0
        var root_label: &[u8] = path.file_name(target)
        if fs.is_dir(target) {
            root_label = ""
        }
        walk_path(target, root_label, true, false, "", &mut totals, &mut matches, 0)?
        out_print("TOTAL files=")?
        out_print_i64(totals.files)?
        out_print(" lines=")?
        out_print_i64(totals.lines)?
        out_print(" words=")?
        out_print_i64(totals.words)?
        out_print(" bytes=")?
        out_println_i64(totals.bytes)?
        return ()
    }

    if argc != 4 {
        return AppError.BadArgs
    }
    let query_arg = require_arg(3)?
    let query = bytes_view(query_arg)
    if query.len() == 0 {
        return AppError.EmptyQuery
    }
    if !fs.exists(target) {
        return AppError.MissingPath
    }

    var totals = stats.empty_totals()
    var matches: i64 = 0
    var root_label: &[u8] = path.file_name(target)
    if fs.is_dir(target) {
        root_label = ""
    }
    walk_path(target, root_label, is_summary, true, query, &mut totals, &mut matches, 0)?

    if is_summary {
        out_print("TOTAL files=")?
        out_print_i64(totals.files)?
        out_print(" lines=")?
        out_print_i64(totals.lines)?
        out_print(" words=")?
        out_print_i64(totals.words)?
        out_print(" bytes=")?
        out_print_i64(totals.bytes)?
        out_print(" matches=")?
        out_println_i64(matches)?
    } else {
        out_print("TOTAL_MATCHES ")?
        out_println_i64(matches)?
    }
    return ()
}

fn main() -> i32 {
    match run_cli() {
        .ok(_) => { return 0 },
        .err(e) => {
            match e {
                AppError.BadArgs() => {
                    let _ = io.eprintln("usage: text-stats-and-file-search <stats|search|summary> <path> [query]")
                    return 2
                },
                AppError.EmptyQuery() => {
                    let _ = io.eprintln("error: search query must not be empty")
                    return 2
                },
                AppError.MissingPath() => {
                    let _ = io.eprintln("error: path does not exist")
                    return 1
                },
                AppError.InvalidUtf8() => {
                    let _ = io.eprintln("error: invalid UTF-8 input")
                    return 1
                },
                AppError.Allocation() => {
                    let _ = io.eprintln("error: allocation failed")
                    return 1
                },
                AppError.Io() => {
                    let _ = io.eprintln("error: filesystem I/O failure")
                    return 1
                },
            }
        }
    }
}
