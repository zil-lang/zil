// Todo list command implementations: add, list, done, remove, stats.
import std.io
import std.string
import storage

fn decode_title(raw: &[u8], byte_limit: i64) -> storage.TodoError!string.String {
    match string.from_utf8(raw, byte_limit) {
        .ok(s) => { return s },
        .err(_) => { return storage.TodoError.InvalidUtf8 }
    }
}

fn trim_title(src: &string.String, byte_limit: i64) -> storage.TodoError!string.String {
    match string.trim(src, byte_limit) {
        .ok(s) => { return s },
        .err(_) => { return storage.TodoError.Allocation }
    }
}

fn out_print(s: &[u8]) -> storage.TodoError!void {
    match io.print(s) {
        .ok(_) => { return () },
        .err(_) => { return storage.TodoError.Io }
    }
}

fn out_println(s: &[u8]) -> storage.TodoError!void {
    match io.println(s) {
        .ok(_) => { return () },
        .err(_) => { return storage.TodoError.Io }
    }
}

fn out_print_i64(n: i64) -> storage.TodoError!void {
    match io.print_i64(n) {
        .ok(_) => { return () },
        .err(_) => { return storage.TodoError.Io }
    }
}

fn out_println_i64(n: i64) -> storage.TodoError!void {
    match io.println_i64(n) {
        .ok(_) => { return () },
        .err(_) => { return storage.TodoError.Io }
    }
}

pub fn add_todo(list: &mut storage.TodoList, raw_title: &[u8], byte_limit: i64) -> storage.TodoError!i64 {
    let parsed = decode_title(raw_title, byte_limit)?
    let trimmed = trim_title(&parsed, byte_limit)?
    if string.is_empty(&trimmed) {
        return storage.TodoError.EmptyTitle
    }
    return storage.append_item(list, trimmed)
}

pub fn mark_done(list: &mut storage.TodoList, target_id: i64) -> storage.TodoError!void {
    return storage.mark_item_done(list, target_id)
}

pub fn remove_todo(list: &mut storage.TodoList, target_id: i64) -> storage.TodoError!void {
    return storage.remove_item_by_id(list, target_id)
}

pub fn print_list(list: &storage.TodoList) -> storage.TodoError!void {
    let count = storage.item_count(list)
    if count == 0 {
        return out_println("EMPTY")
    }
    var idx: i64 = 0
    while idx < count {
        let item = storage.item_at(list, idx)
        if item.done {
            out_print("[x] ")?
        } else {
            out_print("[ ] ")?
        }
        out_print_i64(item.id)?
        out_print(": ")?
        out_println(string.as_bytes(&item.title))?
        idx += 1
    }
    return ()
}

pub fn print_stats(list: &storage.TodoList) -> storage.TodoError!void {
    let total = storage.item_count(list)
    var done_count: i64 = 0
    var idx: i64 = 0
    while idx < total {
        let item = storage.item_at(list, idx)
        if item.done {
            done_count += 1
        }
        idx += 1
    }
    let pending = total - done_count
    out_print("STATS total=")?
    out_print_i64(total)?
    out_print(" done=")?
    out_print_i64(done_count)?
    out_print(" pending=")?
    out_println_i64(pending)?
    return ()
}
