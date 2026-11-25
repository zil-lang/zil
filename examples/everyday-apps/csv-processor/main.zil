// Everyday CLI application: CSV parsing, generic filtering, aggregation, and formatting.
// Usage:
//   csv-processor normalize <input.csv> <output.csv>
//   csv-processor agg <input.csv> <agg_col>
//   csv-processor filter-agg <input.csv> <filter_col> <filter_val> <agg_col> [output.csv]
import std.env
import std.fs
import std.io
import std.string
import parser
import agg

enum CliError {
    BadArgs,
    Csv(parser.CsvError),
}

fn require_arg(index: i64) -> CliError!Managed<ByteBuffer> {
    match env.arg(index) {
        Option.Some(v) => { return v },
        Option.None() => { return CliError.BadArgs }
    }
}

fn read_input_bytes(path: &[u8]) -> parser.CsvError!ByteBuffer
    contract { result(create) }
{
    match fs.read_to_bytes(path) {
        .ok(b) => { return give b },
        .err(_) => { return parser.CsvError.Io }
    }
}

fn write_output_bytes(path: &[u8], data: &[u8]) -> parser.CsvError!void {
    match fs.write_bytes(path, data) {
        .ok(_) => { return () },
        .err(_) => { return parser.CsvError.Io }
    }
}

fn out_print(s: &[u8]) -> parser.CsvError!void {
    match io.print(s) {
        .ok(_) => { return () },
        .err(_) => { return parser.CsvError.Io }
    }
}

fn out_print_i64(n: i64) -> parser.CsvError!void {
    match io.print_i64(n) {
        .ok(_) => { return () },
        .err(_) => { return parser.CsvError.Io }
    }
}

fn out_println_i64(n: i64) -> parser.CsvError!void {
    match io.println_i64(n) {
        .ok(_) => { return () },
        .err(_) => { return parser.CsvError.Io }
    }
}

fn decode_utf8(raw: &[u8], byte_limit: i64) -> parser.CsvError!string.String {
    match string.from_utf8(raw, byte_limit) {
        .ok(s) => { return s },
        .err(_) => { return parser.CsvError.InvalidUtf8 }
    }
}

fn print_summary(summary: agg.NumericSummary) -> parser.CsvError!void {
    out_print("SUMMARY count=")?
    out_print_i64(summary.count)?
    out_print(" sum=")?
    out_print_i64(summary.sum)?
    out_print(" min=")?
    out_print_i64(summary.min)?
    out_print(" max=")?
    out_println_i64(summary.max)?
    return ()
}

fn load_table(path: &[u8], byte_limit: i64) -> parser.CsvError!parser.CsvTable {
    let raw = read_input_bytes(path)?
    return parser.parse_csv(bytes_view(raw), byte_limit)
}

fn run_normalize(in_path: &[u8], out_path: &[u8], byte_limit: i64) -> parser.CsvError!void {
    let table = load_table(in_path, byte_limit)?
    let formatted = parser.format_csv(&table)?
    return write_output_bytes(out_path, bytes_view(formatted))
}

fn run_agg(in_path: &[u8], col_name: &[u8], byte_limit: i64) -> parser.CsvError!void {
    let table = load_table(in_path, byte_limit)?
    let all_filter = agg.AllFilter { active: true }
    let selected = agg.filter_rows(&table, &all_filter, byte_limit)?
    let col_idx = parser.find_column(&selected, col_name)?
    let summary = agg.aggregate_i64(&selected, col_idx, byte_limit)?
    return print_summary(summary)
}

fn run_filter_agg(
    in_path: &[u8],
    filter_col_name: &[u8],
    filter_val: &[u8],
    agg_col_name: &[u8],
    has_out: bool,
    out_path: &[u8],
    byte_limit: i64,
) -> parser.CsvError!void {
    let table = load_table(in_path, byte_limit)?
    let filter_col = parser.find_column(&table, filter_col_name)?
    let agg_col = parser.find_column(&table, agg_col_name)?
    let expected_str = decode_utf8(filter_val, byte_limit)?
    let eq_filter = agg.EqualsFilter {
        col: filter_col,
        expected: expected_str,
    }
    let filtered = agg.filter_rows(&table, &eq_filter, byte_limit)?
    let summary = agg.aggregate_i64(&filtered, agg_col, byte_limit)?
    let formatted = parser.format_csv(&filtered)?
    if has_out {
        write_output_bytes(out_path, bytes_view(formatted))?
    } else {
        out_print(bytes_view(formatted))?
    }
    return print_summary(summary)
}

fn run_cli() -> CliError!void {
    let argc = env.arg_count()
    if argc < 4 {
        return CliError.BadArgs
    }
    let cmd_arg = require_arg(1)?
    let in_arg = require_arg(2)?
    let cmd = bytes_view(cmd_arg)
    let in_path = bytes_view(in_arg)
    let byte_limit: i64 = 16777216

    if parser.bytes_eq(cmd, "normalize") {
        if argc != 4 {
            return CliError.BadArgs
        }
        let out_arg = require_arg(3)?
        match run_normalize(in_path, bytes_view(out_arg), byte_limit) {
            .ok(_) => { return () },
            .err(e) => { return CliError.Csv(e) }
        }
    }

    if parser.bytes_eq(cmd, "agg") {
        if argc != 4 {
            return CliError.BadArgs
        }
        let col_arg = require_arg(3)?
        match run_agg(in_path, bytes_view(col_arg), byte_limit) {
            .ok(_) => { return () },
            .err(e) => { return CliError.Csv(e) }
        }
    }

    if parser.bytes_eq(cmd, "filter-agg") {
        if argc != 6 && argc != 7 {
            return CliError.BadArgs
        }
        let filter_col_arg = require_arg(3)?
        let filter_val_arg = require_arg(4)?
        let agg_col_arg = require_arg(5)?
        if argc == 7 {
            let out_arg = require_arg(6)?
            match run_filter_agg(
                in_path,
                bytes_view(filter_col_arg),
                bytes_view(filter_val_arg),
                bytes_view(agg_col_arg),
                true,
                bytes_view(out_arg),
                byte_limit,
            ) {
                .ok(_) => { return () },
                .err(e) => { return CliError.Csv(e) }
            }
        }
        match run_filter_agg(
            in_path,
            bytes_view(filter_col_arg),
            bytes_view(filter_val_arg),
            bytes_view(agg_col_arg),
            false,
            "",
            byte_limit,
        ) {
            .ok(_) => { return () },
            .err(e) => { return CliError.Csv(e) }
        }
    }

    return CliError.BadArgs
}

fn main() -> i32 {
    match run_cli() {
        .ok(_) => { return 0 },
        .err(err) => {
            match err {
                CliError.BadArgs() => {
                    let _ = io.eprintln("usage: csv-processor <normalize|agg|filter-agg> ...")
                    return 2
                },
                CliError.Csv(e) => {
                    match e {
                        parser.CsvError.EmptyInput() => { let _ = io.eprintln("error: empty CSV input") },
                        parser.CsvError.UnterminatedQuote() => { let _ = io.eprintln("error: unterminated quoted CSV field") },
                        parser.CsvError.InvalidQuoteEscape() => { let _ = io.eprintln("error: invalid CSV quoting") },
                        parser.CsvError.ColumnCountMismatch() => { let _ = io.eprintln("error: CSV row column count mismatch") },
                        parser.CsvError.MissingColumn() => { let _ = io.eprintln("error: CSV column not found") },
                        parser.CsvError.InvalidNumber() => { let _ = io.eprintln("error: invalid integer cell in numeric column") },
                        parser.CsvError.InvalidUtf8() => { let _ = io.eprintln("error: invalid UTF-8 in CSV") },
                        parser.CsvError.Capacity() => { let _ = io.eprintln("error: CSV capacity limit exceeded") },
                        parser.CsvError.Allocation() => { let _ = io.eprintln("error: allocation failed") },
                        parser.CsvError.Io() => { let _ = io.eprintln("error: CSV file I/O failed") },
                    }
                    return 1
                },
            }
        }
    }
}
