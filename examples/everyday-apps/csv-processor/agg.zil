// Generic row filtering and numeric column aggregation for CSV tables.
import std.int
import std.string
import parser

pub trait RowFilter {
    fn matches(self: &Self, row: &parser.CsvRow) -> bool;
}

pub struct AllFilter {
    pub active: bool;
}

impl RowFilter for AllFilter {
    fn matches(self: &Self, row: &parser.CsvRow) -> bool {
        return self.active && parser.row_len(row) >= 0
    }
}

pub package EqualsFilter {
    pub col: i64;
    pub expected: string.String;
}

impl RowFilter for EqualsFilter {
    fn matches(self: &Self, row: &parser.CsvRow) -> bool {
        if self.col < 0 || self.col >= parser.row_len(row) {
            return false
        }
        let cell = parser.row_field_bytes(row, self.col)
        return parser.bytes_eq(cell, string.as_bytes(&self.expected))
    }
}

pub struct NumericSummary {
    pub count: i64;
    pub sum: i64;
    pub min: i64;
    pub max: i64;
}

fn trim_str(src: &string.String, byte_limit: i64) -> parser.CsvError!string.String {
    match string.trim(src, byte_limit) {
        .ok(s) => { return s },
        .err(_) => { return parser.CsvError.Capacity }
    }
}

fn parse_cell_i64(raw: &[u8]) -> parser.CsvError!i64 {
    match int.parse_i64(raw) {
        .ok(n) => { return n },
        .err(_) => { return parser.CsvError.InvalidNumber }
    }
}

pub fn filter_rows<F: RowFilter>(
    table: &parser.CsvTable,
    filter: &F,
    byte_limit: i64,
) -> parser.CsvError!parser.CsvTable {
    let out_headers: Store<string.String> = store_new()
    var h_idx: i64 = 0
    let h_count = parser.header_count(table)
    while h_idx < h_count {
        let copy = parser.header_clone(table, h_idx, byte_limit)?
        parser.push_string(out_headers, copy)?
        h_idx += 1
    }

    let out_rows: Store<parser.CsvRow> = store_new()
    var r_idx: i64 = 0
    let r_count = parser.row_count(table)
    while r_idx < r_count {
        let row = parser.row_at(table, r_idx)
        if filter.matches(row) {
            let fields: Store<string.String> = store_new()
            var c_idx: i64 = 0
            let c_count = parser.row_len(row)
            while c_idx < c_count {
                let copy = parser.row_field_clone(row, c_idx, byte_limit)?
                parser.push_string(fields, copy)?
                c_idx += 1
            }
            parser.push_row(out_rows, parser.make_row(give fields))?
        }
        r_idx += 1
    }

    return parser.make_table(give out_headers, give out_rows)
}

pub fn aggregate_i64(
    table: &parser.CsvTable,
    col: i64,
    byte_limit: i64,
) -> parser.CsvError!NumericSummary {
    if col < 0 || col >= parser.header_count(table) {
        return parser.CsvError.MissingColumn
    }
    var count: i64 = 0
    var sum: i64 = 0
    var min_val: i64 = 0
    var max_val: i64 = 0
    var r_idx: i64 = 0
    let r_count = parser.row_count(table)
    while r_idx < r_count {
        let row = parser.row_at(table, r_idx)
        let raw_cell = parser.row_field_str(row, col)
        let trimmed = trim_str(raw_cell, byte_limit)?
        let val = parse_cell_i64(string.as_bytes(&trimmed))?
        if count == 0 {
            min_val = val
            max_val = val
        } else {
            if val < min_val {
                min_val = val
            }
            if val > max_val {
                max_val = val
            }
        }
        sum += val
        count += 1
        r_idx += 1
    }
    return NumericSummary {
        count: count,
        sum: sum,
        min: min_val,
        max: max_val,
    }
}
