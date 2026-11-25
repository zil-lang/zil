// Persistent atomic file storage for CLI Todo items.
import std.fs
import std.int
import std.path
import std.store
import std.string

pub enum TodoError {
    BadArgs,
    InvalidId,
    NotFound,
    EmptyTitle,
    CorruptStore,
    InvalidUtf8,
    Allocation,
    Io,
}

pub package TodoItem {
    pub id: i64;
    pub done: bool;
    pub title: string.String;
}

pub package TodoList {
    pub next_id: i64;
    manages items: Store<TodoItem>;
}

pub fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
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

fn push_byte(buf: ByteBuffer, byte: u8) -> TodoError!void {
    let cur = bytes_len(buf)
    if cur >= bytes_capacity(buf) {
        var next: i64 = 32
        if cur >= 32 {
            if cur > 4611686018427387903 {
                return TodoError.Allocation
            }
            next = cur * 2
        }
        if !bytes_reserve(buf, next) {
            return TodoError.Allocation
        }
    }
    if !bytes_push(buf, byte) {
        return TodoError.Allocation
    }
    return ()
}

fn push_slice(buf: ByteBuffer, slice: &[u8]) -> TodoError!void {
    var at: i64 = 0
    while at < slice.len() {
        push_byte(buf, slice[at])?
        at += 1
    }
    return ()
}

pub fn push_item(items: Store<TodoItem>, item: TodoItem) -> TodoError!void {
    match store.push_growing(items, item) {
        .ok(_) => { return () },
        .err(_) => { return TodoError.Allocation }
    }
}

pub fn item_count(list: &TodoList) -> i64 {
    return store_len(list.items)
}

pub fn item_at(list: &TodoList, index: i64) -> &TodoItem {
    return store_at(list.items, index)
}

pub fn append_item(list: &mut TodoList, title: string.String) -> TodoError!i64 {
    let assigned_id = list.next_id
    list.next_id += 1
    let item = TodoItem {
        id: assigned_id,
        done: false,
        title: title,
    }
    push_item(list.items, item)?
    return assigned_id
}

pub fn mark_item_done(list: &mut TodoList, target_id: i64) -> TodoError!void {
    if target_id <= 0 {
        return TodoError.InvalidId
    }
    var idx: i64 = 0
    let count = store_len(list.items)
    while idx < count {
        let item = store_at_mut(list.items, idx)
        if item.id == target_id {
            item.done = true
            return ()
        }
        idx += 1
    }
    return TodoError.NotFound
}

pub fn remove_item_by_id(list: &mut TodoList, target_id: i64) -> TodoError!void {
    if target_id <= 0 {
        return TodoError.InvalidId
    }
    var found: i64 = -1
    var idx: i64 = 0
    let count = store_len(list.items)
    while idx < count {
        let item = store_at(list.items, idx)
        if item.id == target_id {
            found = idx
            break
        }
        idx += 1
    }
    if found < 0 {
        return TodoError.NotFound
    }
    match store.remove::<TodoItem>(list.items, found) {
        .ok(_) => { return () },
        .err(_) => { return TodoError.Allocation }
    }
}

fn parse_store_i64(raw: &[u8]) -> TodoError!i64 {
    match int.parse_i64(raw) {
        .ok(n) => { return n },
        .err(_) => { return TodoError.CorruptStore }
    }
}

fn read_store_bytes(store_path: &[u8]) -> TodoError!ByteBuffer
    contract { result(create) }
{
    match fs.read_to_bytes(store_path) {
        .ok(b) => { return give b },
        .err(_) => { return TodoError.Io }
    }
}

fn decode_store_utf8(raw: &[u8], byte_limit: i64) -> TodoError!string.String {
    match string.from_utf8(raw, byte_limit) {
        .ok(s) => { return s },
        .err(_) => { return TodoError.CorruptStore }
    }
}

fn split_store_lines(text: &string.String, byte_limit: i64) -> TodoError!Store<string.String>
    contract { result(create) }
{
    match string.lines(text, byte_limit) {
        .ok(ls) => { return give ls },
        .err(_) => { return TodoError.Allocation }
    }
}

fn escape_title(out: ByteBuffer, raw: &[u8]) -> TodoError!void {
    var at: i64 = 0
    while at < raw.len() {
        let ch = raw[at]
        if ch == 92u8 {
            push_slice(out, "\\\\")?
        } else if ch == 9u8 {
            push_slice(out, "\\t")?
        } else if ch == 10u8 {
            push_slice(out, "\\n")?
        } else if ch == 13u8 {
            push_slice(out, "\\r")?
        } else {
            push_byte(out, ch)?
        }
        at += 1
    }
    return ()
}

fn unescape_title(raw: &[u8], byte_limit: i64) -> TodoError!string.String {
    let out = bytes_new()
    var at: i64 = 0
    while at < raw.len() {
        let ch = raw[at]
        if ch == 92u8 {
            if at + 1 >= raw.len() {
                return TodoError.CorruptStore
            }
            let esc = raw[at + 1]
            if esc == 92u8 {
                push_byte(out, 92u8)?
            } else if esc == 116u8 {
                push_byte(out, 9u8)?
            } else if esc == 110u8 {
                push_byte(out, 10u8)?
            } else if esc == 114u8 {
                push_byte(out, 13u8)?
            } else {
                return TodoError.CorruptStore
            }
            at += 2
        } else {
            push_byte(out, ch)?
            at += 1
        }
    }
    match string.from_utf8(bytes_view(out), byte_limit) {
        .ok(s) => { return s },
        .err(_) => { return TodoError.InvalidUtf8 }
    }
}

fn parse_item_line(line: &[u8], byte_limit: i64) -> TodoError!TodoItem {
    var first_tab: i64 = -1
    var second_tab: i64 = -1
    var at: i64 = 0
    while at < line.len() {
        if line[at] == 9u8 {
            if first_tab < 0 {
                first_tab = at
            } else {
                second_tab = at
                break
            }
        }
        at += 1
    }
    if first_tab <= 0 || second_tab <= first_tab + 1 {
        return TodoError.CorruptStore
    }
    let id = parse_store_i64(&line[0..first_tab])?
    if id <= 0 {
        return TodoError.CorruptStore
    }
    let done_slice = &line[first_tab + 1..second_tab]
    var done = false
    if bytes_eq(done_slice, "1") {
        done = true
    } else if bytes_eq(done_slice, "0") {
        done = false
    } else {
        return TodoError.CorruptStore
    }
    let title = unescape_title(&line[second_tab + 1..line.len()], byte_limit)?
    return TodoItem {
        id: id,
        done: done,
        title: title,
    }
}

pub fn load(store_path: &[u8], byte_limit: i64) -> TodoError!TodoList {
    if !fs.exists(store_path) {
        return TodoList {
            next_id: 1,
            items: store_new(),
        }
    }
    let raw = read_store_bytes(store_path)?
    let text = decode_store_utf8(bytes_view(raw), byte_limit)?
    let split_lines = split_store_lines(&text, byte_limit)?
    let line_count = store_len(split_lines)
    if line_count == 0 {
        return TodoList {
            next_id: 1,
            items: store_new(),
        }
    }
    let header_line = string.as_bytes(store_at(split_lines, 0))
    if header_line.len() < 6 || !bytes_eq(&header_line[0..5], "NEXT ") {
        return TodoError.CorruptStore
    }
    let next_id = parse_store_i64(&header_line[5..header_line.len()])?
    if next_id < 1 {
        return TodoError.CorruptStore
    }
    let items: Store<TodoItem> = store_new()
    var idx: i64 = 1
    while idx < line_count {
        let line_bytes = string.as_bytes(store_at(split_lines, idx))
        if line_bytes.len() > 0 {
            let item = parse_item_line(line_bytes, byte_limit)?
            push_item(items, item)?
        }
        idx += 1
    }
    return TodoList {
        next_id: next_id,
        items: give items,
    }
}

fn serialize_list(list: &TodoList) -> TodoError!ByteBuffer
    contract { result(create) }
{
    let out = bytes_new()
    push_slice(out, "NEXT ")?
    let next_dec = int.format_i64(list.next_id)
    push_slice(out, &next_dec.bytes[0..next_dec.len])?
    push_byte(out, 10u8)?

    var idx: i64 = 0
    let count = store_len(list.items)
    while idx < count {
        let item = store_at(list.items, idx)
        let id_dec = int.format_i64(item.id)
        push_slice(out, &id_dec.bytes[0..id_dec.len])?
        push_byte(out, 9u8)?
        if item.done {
            push_byte(out, 49u8)?
        } else {
            push_byte(out, 48u8)?
        }
        push_byte(out, 9u8)?
        escape_title(out, string.as_bytes(&item.title))?
        push_byte(out, 10u8)?
        idx += 1
    }
    return give out
}

fn make_temp_path(store_path: &[u8]) -> TodoError!ByteBuffer
    contract { result(create) }
{
    let tmp = bytes_new()
    push_slice(tmp, store_path)?
    push_slice(tmp, ".tmp")?
    return give tmp
}

pub fn save_atomic(store_path: &[u8], list: &TodoList) -> TodoError!void {
    let parent_dir = path.parent(store_path)
    if parent_dir.len() > 0 {
        match fs.mkdir_all(parent_dir) {
            .ok(_) => {},
            .err(_) => { return TodoError.Io }
        }
    }
    let payload = serialize_list(list)?
    let tmp_path = make_temp_path(store_path)?
    match fs.write_bytes(bytes_view(tmp_path), bytes_view(payload)) {
        .ok(_) => {},
        .err(_) => {
            let _ = fs.remove_file(bytes_view(tmp_path))
            return TodoError.Io
        }
    }
    match fs.rename(bytes_view(tmp_path), store_path) {
        .ok(_) => { return () },
        .err(_) => {
            let _ = fs.remove_file(bytes_view(tmp_path))
            return TodoError.Io
        }
    }
}
