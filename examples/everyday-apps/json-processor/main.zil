// Everyday CLI application: JSON validation, path querying, and deterministic round-trip serialization.
// Usage:
//   json-processor validate <file.json>
//   json-processor format <file.json> [out.json]
//   json-processor query <file.json> <path>
import std.env
import std.fs
import std.io
import lexer
import value

enum CliError {
    BadArgs,
    Json(lexer.JsonError),
}

fn require_arg(index: i64) -> CliError!Managed<ByteBuffer> {
    match env.arg(index) {
        Option.Some(v) => { return v },
        Option.None() => { return CliError.BadArgs }
    }
}

fn read_json_bytes(path: &[u8]) -> lexer.JsonError!ByteBuffer
    contract { result(create) }
{
    match fs.read_to_bytes(path) {
        .ok(b) => { return give b },
        .err(_) => { return lexer.JsonError.Io }
    }
}

fn write_json_bytes(path: &[u8], data: &[u8]) -> lexer.JsonError!void {
    match fs.write_bytes(path, data) {
        .ok(_) => { return () },
        .err(_) => { return lexer.JsonError.Io }
    }
}

fn out_println(s: &[u8]) -> lexer.JsonError!void {
    match io.println(s) {
        .ok(_) => { return () },
        .err(_) => { return lexer.JsonError.Io }
    }
}

fn load_document(path: &[u8], byte_limit: i64) -> lexer.JsonError!value.JsonDocument {
    let raw = read_json_bytes(path)?
    return value.parse(bytes_view(raw), byte_limit)
}

fn run_validate(file_path: &[u8], byte_limit: i64) -> lexer.JsonError!void {
    let doc = load_document(file_path, byte_limit)?
    if doc.root < 0 {
        return lexer.JsonError.UnexpectedToken
    }
    return out_println("VALID")
}

fn run_format(
    file_path: &[u8],
    has_out: bool,
    out_path: &[u8],
    byte_limit: i64,
) -> lexer.JsonError!void {
    let doc = load_document(file_path, byte_limit)?
    let serialized = value.serialize_node(&doc, doc.root)?
    if has_out {
        return write_json_bytes(out_path, bytes_view(serialized))
    }
    return out_println(bytes_view(serialized))
}

fn run_query(file_path: &[u8], query: &[u8], byte_limit: i64) -> lexer.JsonError!void {
    let doc = load_document(file_path, byte_limit)?
    let target_idx = value.query_path(&doc, query)?
    let serialized = value.serialize_node(&doc, target_idx)?
    return out_println(bytes_view(serialized))
}

fn run_cli() -> CliError!void {
    let argc = env.arg_count()
    if argc < 3 {
        return CliError.BadArgs
    }
    let cmd_arg = require_arg(1)?
    let file_arg = require_arg(2)?
    let cmd = bytes_view(cmd_arg)
    let file_path = bytes_view(file_arg)
    let byte_limit: i64 = 16777216

    if lexer.bytes_eq(cmd, "validate") {
        if argc != 3 {
            return CliError.BadArgs
        }
        match run_validate(file_path, byte_limit) {
            .ok(_) => { return () },
            .err(e) => { return CliError.Json(e) }
        }
    }

    if lexer.bytes_eq(cmd, "format") {
        if argc != 3 && argc != 4 {
            return CliError.BadArgs
        }
        if argc == 4 {
            let out_arg = require_arg(3)?
            match run_format(file_path, true, bytes_view(out_arg), byte_limit) {
                .ok(_) => { return () },
                .err(e) => { return CliError.Json(e) }
            }
        }
        match run_format(file_path, false, "", byte_limit) {
            .ok(_) => { return () },
            .err(e) => { return CliError.Json(e) }
        }
    }

    if lexer.bytes_eq(cmd, "query") {
        if argc != 4 {
            return CliError.BadArgs
        }
        let query_arg = require_arg(3)?
        match run_query(file_path, bytes_view(query_arg), byte_limit) {
            .ok(_) => { return () },
            .err(e) => { return CliError.Json(e) }
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
                    let _ = io.eprintln("usage: json-processor <validate|format|query> <file.json> [arg]")
                    return 2
                },
                CliError.Json(e) => {
                    match e {
                        lexer.JsonError.UnexpectedChar() => { let _ = io.eprintln("error: unexpected character in JSON") },
                        lexer.JsonError.UnterminatedString() => { let _ = io.eprintln("error: unterminated JSON string") },
                        lexer.JsonError.InvalidEscape() => { let _ = io.eprintln("error: invalid escape in JSON string") },
                        lexer.JsonError.InvalidNumber() => { let _ = io.eprintln("error: invalid JSON number") },
                        lexer.JsonError.UnexpectedToken() => { let _ = io.eprintln("error: unexpected JSON token") },
                        lexer.JsonError.TrailingTokens() => { let _ = io.eprintln("error: trailing data after JSON root") },
                        lexer.JsonError.DepthLimit() => { let _ = io.eprintln("error: JSON nesting depth limit exceeded") },
                        lexer.JsonError.KeyNotFound() => { let _ = io.eprintln("error: JSON object key not found") },
                        lexer.JsonError.IndexOutOfBounds() => { let _ = io.eprintln("error: JSON array index out of bounds") },
                        lexer.JsonError.InvalidQuery() => { let _ = io.eprintln("error: invalid JSON query path") },
                        lexer.JsonError.InvalidUtf8() => { let _ = io.eprintln("error: invalid UTF-8 in JSON") },
                        lexer.JsonError.Allocation() => { let _ = io.eprintln("error: allocation failed") },
                        lexer.JsonError.Io() => { let _ = io.eprintln("error: JSON file I/O failed") },
                    }
                    return 1
                },
            }
        }
    }
}
