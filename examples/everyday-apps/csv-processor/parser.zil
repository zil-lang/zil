// RFC-4180 CSV parser, column lookup, and canonical formatter.
import std.store
import std.string

pub enum CsvError {
    EmptyInput,
    UnterminatedQuote,
    InvalidQuoteEscape,
    ColumnCountMismatch,
    MissingColumn,
    InvalidNumber,
    InvalidUtf8,
    Capacity,
    Allocation,
    Io,
}

pub package CsvRow {
    manages fields: Store<string.String>;
}

pub package CsvTable {
    manages headers: Store<string.String>;
    manages rows: Store<CsvRow>;
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

pub fn row_len(row: &CsvRow) -> i64 {
    return store_len(row.fields)
}

pub fn row_field_bytes(row: &CsvRow, index: i64) -> &[u8] {
    let cell = store_at(row.fields, index)
    return string.as_bytes(cell)
}

pub fn row_field_str(row: &CsvRow, index: i64) -> &string.String {
    return store_at(row.fields, index)
}

pub fn row_field_clone(row: &CsvRow, index: i64, byte_limit: i64) -> CsvError!string.String {
    if index < 0 || index >= store_len(row.fields) {
        return CsvError.MissingColumn
    }
    let cell = store_at(row.fields, index)
    match string.clone(cell, byte_limit) {
        .ok(copy) => { return copy },
        .err(_) => { return CsvError.Capacity }
    }
}

pub fn header_count(table: &CsvTable) -> i64 {
    return store_len(table.headers)
}

pub fn header_clone(table: &CsvTable, index: i64, byte_limit: i64) -> CsvError!string.String {
    if index < 0 || index >= store_len(table.headers) {
        return CsvError.MissingColumn
    }
    let hdr = store_at(table.headers, index)
    match string.clone(hdr, byte_limit) {
        .ok(copy) => { return copy },
        .err(_) => { return CsvError.Capacity }
    }
}

pub fn row_count(table: &CsvTable) -> i64 {
    return store_len(table.rows)
}

pub fn row_at(table: &CsvTable, index: i64) -> &CsvRow {
    return store_at(table.rows, index)
}

pub fn make_row(give fields: Store<string.String>) -> CsvRow {
    return CsvRow { fields: give fields }
}

pub fn make_table(give headers: Store<string.String>, give rows: Store<CsvRow>) -> CsvTable {
    return CsvTable {
        headers: give headers,
        rows: give rows,
    }
}

pub fn push_byte(buf: ByteBuffer, byte: u8) -> CsvError!void {
    let cur = bytes_len(buf)
    if cur >= bytes_capacity(buf) {
        var next: i64 = 16
        if cur >= 16 {
            if cur > 4611686018427387903 {
                return CsvError.Capacity
            }
            next = cur * 2
        }
        if !bytes_reserve(buf, next) {
            return CsvError.Allocation
        }
    }
    if !bytes_push(buf, byte) {
        return CsvError.Allocation
    }
    return ()
}

pub fn push_string(items: Store<string.String>, value: string.String) -> CsvError!void {
    match store.push_growing(items, value) {
        .ok(_) => { return () },
        .err(_) => { return CsvError.Allocation }
    }
}

pub fn push_row(items: Store<CsvRow>, row: CsvRow) -> CsvError!void {
    match store.push_growing(items, row) {
        .ok(_) => { return () },
        .err(_) => { return CsvError.Allocation }
    }
}

fn decode_cell(raw: &[u8], byte_limit: i64) -> CsvError!string.String {
    match string.from_utf8(raw, byte_limit) {
        .ok(s) => { return s },
        .err(_) => { return CsvError.InvalidUtf8 }
    }
}

fn parse_record(input: &[u8], pos: &mut i64, byte_limit: i64) -> CsvError!Store<string.String>
    contract { result(create) }
{
    let fields: Store<string.String> = store_new()
    loop {
        let cell_buf = bytes_new()
        let at = *pos
        if at < input.len() && input[at] == 34u8 {
            // Quoted field.
            var cursor = at + 1
            var closed = false
            while cursor < input.len() {
                let ch = input[cursor]
                if ch == 34u8 {
                    if cursor + 1 < input.len() && input[cursor + 1] == 34u8 {
                        push_byte(cell_buf, 34u8)?
                        cursor += 2
                    } else {
                        cursor += 1
                        closed = true
                        break
                    }
                } else {
                    push_byte(cell_buf, ch)?
                    cursor += 1
                }
            }
            if !closed {
                return CsvError.UnterminatedQuote
            }
            *pos = cursor
            if cursor < input.len() {
                let next = input[cursor]
                if next != 44u8 && next != 10u8 && next != 13u8 {
                    return CsvError.InvalidQuoteEscape
                }
            }
        } else {
            // Unquoted field.
            var cursor = at
            while cursor < input.len() {
                let ch = input[cursor]
                if ch == 44u8 || ch == 10u8 || ch == 13u8 {
                    break
                }
                if ch == 34u8 {
                    return CsvError.InvalidQuoteEscape
                }
                push_byte(cell_buf, ch)?
                cursor += 1
            }
            *pos = cursor
        }

        let cell_str = decode_cell(bytes_view(cell_buf), byte_limit)?
        push_string(fields, cell_str)?

        let end = *pos
        if end >= input.len() {
            break
        }
        let delim = input[end]
        if delim == 44u8 {
            *pos = end + 1
        } else if delim == 13u8 {
            if end + 1 < input.len() && input[end + 1] == 10u8 {
                *pos = end + 2
            } else {
                *pos = end + 1
            }
            break
        } else if delim == 10u8 {
            *pos = end + 1
            break
        } else {
            return CsvError.InvalidQuoteEscape
        }
    }
    return give fields
}

pub fn parse_csv(input: &[u8], byte_limit: i64) -> CsvError!CsvTable {
    if input.len() == 0 {
        return CsvError.EmptyInput
    }
    var pos: i64 = 0
    let headers = parse_record(input, &mut pos, byte_limit)?
    let col_count = store_len(headers)
    if col_count == 0 {
        return CsvError.EmptyInput
    }
    let rows: Store<CsvRow> = store_new()
    while pos < input.len() {
        let fields = parse_record(input, &mut pos, byte_limit)?
        if store_len(fields) != col_count {
            return CsvError.ColumnCountMismatch
        }
        push_row(rows, CsvRow { fields: give fields })?
    }
    return CsvTable {
        headers: give headers,
        rows: give rows,
    }
}

pub fn find_column(table: &CsvTable, name: &[u8]) -> CsvError!i64 {
    var idx: i64 = 0
    let count = store_len(table.headers)
    while idx < count {
        let header = store_at(table.headers, idx)
        if bytes_eq(string.as_bytes(header), name) {
            return idx
        }
        idx += 1
    }
    return CsvError.MissingColumn
}

fn needs_quoting(field: &[u8]) -> bool {
    var at: i64 = 0
    while at < field.len() {
        let ch = field[at]
        if ch == 44u8 || ch == 34u8 || ch == 10u8 || ch == 13u8 {
            return true
        }
        at += 1
    }
    return false
}

fn format_field(out: ByteBuffer, field: &[u8]) -> CsvError!void {
    if needs_quoting(field) {
        push_byte(out, 34u8)?
        var at: i64 = 0
        while at < field.len() {
            let ch = field[at]
            if ch == 34u8 {
                push_byte(out, 34u8)?
                push_byte(out, 34u8)?
            } else {
                push_byte(out, ch)?
            }
            at += 1
        }
        push_byte(out, 34u8)?
    } else {
        var at: i64 = 0
        while at < field.len() {
            push_byte(out, field[at])?
            at += 1
        }
    }
    return ()
}

fn format_fields(out: ByteBuffer, fields: Store<string.String>) -> CsvError!void {
    var idx: i64 = 0
    let count = store_len(fields)
    while idx < count {
        if idx > 0 {
            push_byte(out, 44u8)?
        }
        let cell = store_at(fields, idx)
        format_field(out, string.as_bytes(cell))?
        idx += 1
    }
    push_byte(out, 10u8)?
    return ()
}

pub fn format_csv(table: &CsvTable) -> CsvError!ByteBuffer
    contract { result(create) }
{
    let out = bytes_new()
    format_fields(out, table.headers)?
    var row_idx: i64 = 0
    let row_count = store_len(table.rows)
    while row_idx < row_count {
        let row = store_at(table.rows, row_idx)
        format_fields(out, row.fields)?
        row_idx += 1
    }
    return give out
}
