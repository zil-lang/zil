// Recursive-descent JSON parser, owning tagged AST, path querying, and canonical serializer.
import std.int
import std.store
import std.string
import lexer

pub package JsonField {
    pub key: string.String;
    pub value: i64;
}

pub package JsonArray {
    manages items: Store<i64>;
}

pub package JsonObject {
    manages fields: Store<JsonField>;
}

pub enum JsonValue {
    Null,
    Bool(bool),
    Int(i64),
    Decimal(string.String),
    Str(string.String),
    Array(JsonArray),
    Object(JsonObject),
}

pub package JsonDocument {
    manages nodes: Store<JsonValue>;
    pub root: i64;
}

fn push_node(nodes: Store<JsonValue>, val: JsonValue) -> lexer.JsonError!i64 {
    let idx = store_len(nodes)
    match store.push_growing(nodes, val) {
        .ok(_) => { return idx },
        .err(_) => { return lexer.JsonError.Allocation }
    }
}

fn push_i64(items: Store<i64>, val: i64) -> lexer.JsonError!void {
    match store.push_growing(items, val) {
        .ok(_) => { return () },
        .err(_) => { return lexer.JsonError.Allocation }
    }
}

fn push_field(fields: Store<JsonField>, field: JsonField) -> lexer.JsonError!void {
    match store.push_growing(fields, field) {
        .ok(_) => { return () },
        .err(_) => { return lexer.JsonError.Allocation }
    }
}

fn is_token(tok: lexer.Token, kind: lexer.TokenKind) -> bool {
    match tok.kind {
        lexer.TokenKind.LBrace() => { match kind { lexer.TokenKind.LBrace() => { return true }, _ => { return false } } },
        lexer.TokenKind.RBrace() => { match kind { lexer.TokenKind.RBrace() => { return true }, _ => { return false } } },
        lexer.TokenKind.LBracket() => { match kind { lexer.TokenKind.LBracket() => { return true }, _ => { return false } } },
        lexer.TokenKind.RBracket() => { match kind { lexer.TokenKind.RBracket() => { return true }, _ => { return false } } },
        lexer.TokenKind.Colon() => { match kind { lexer.TokenKind.Colon() => { return true }, _ => { return false } } },
        lexer.TokenKind.Comma() => { match kind { lexer.TokenKind.Comma() => { return true }, _ => { return false } } },
        lexer.TokenKind.True() => { match kind { lexer.TokenKind.True() => { return true }, _ => { return false } } },
        lexer.TokenKind.False() => { match kind { lexer.TokenKind.False() => { return true }, _ => { return false } } },
        lexer.TokenKind.Null() => { match kind { lexer.TokenKind.Null() => { return true }, _ => { return false } } },
        lexer.TokenKind.String() => { match kind { lexer.TokenKind.String() => { return true }, _ => { return false } } },
        lexer.TokenKind.Number() => { match kind { lexer.TokenKind.Number() => { return true }, _ => { return false } } },
        lexer.TokenKind.Eof() => { match kind { lexer.TokenKind.Eof() => { return true }, _ => { return false } } },
    }
}

fn has_decimal_or_exp(slice: &[u8]) -> bool {
    var at: i64 = 0
    while at < slice.len() {
        let ch = slice[at]
        if ch == 46u8 || ch == 101u8 || ch == 69u8 {
            return true
        }
        at += 1
    }
    return false
}

fn decode_decimal(raw: &[u8], byte_limit: i64) -> lexer.JsonError!string.String {
    match string.from_utf8(raw, byte_limit) {
        .ok(s) => { return s },
        .err(_) => { return lexer.JsonError.InvalidUtf8 }
    }
}

fn parse_index(segment: &[u8]) -> lexer.JsonError!i64 {
    match int.parse_i64(segment) {
        .ok(n) => { return n },
        .err(_) => { return lexer.JsonError.InvalidQuery }
    }
}

fn parse_value(
    src: &[u8],
    tokens: Store<lexer.Token>,
    pos: &mut i64,
    nodes: Store<JsonValue>,
    depth: i64,
    byte_limit: i64,
) -> lexer.JsonError!i64 {
    if depth > 64 {
        return lexer.JsonError.DepthLimit
    }
    let idx = *pos
    if idx >= store_len(tokens) {
        return lexer.JsonError.UnexpectedToken
    }
    let tok = *store_at(tokens, idx)
    match tok.kind {
        lexer.TokenKind.Null() => {
            *pos = idx + 1
            return push_node(nodes, JsonValue.Null)
        },
        lexer.TokenKind.True() => {
            *pos = idx + 1
            return push_node(nodes, JsonValue.Bool(true))
        },
        lexer.TokenKind.False() => {
            *pos = idx + 1
            return push_node(nodes, JsonValue.Bool(false))
        },
        lexer.TokenKind.String() => {
            *pos = idx + 1
            let s = lexer.unescape_string(src, tok.start, tok.end, byte_limit)?
            return push_node(nodes, JsonValue.Str(s))
        },
        lexer.TokenKind.Number() => {
            *pos = idx + 1
            let raw = &src[tok.start..tok.end]
            if !has_decimal_or_exp(raw) {
                match int.parse_i64(raw) {
                    .ok(n) => { return push_node(nodes, JsonValue.Int(n)) },
                    .err(_) => {}
                }
            }
            let num_str = decode_decimal(raw, byte_limit)?
            return push_node(nodes, JsonValue.Decimal(num_str))
        },
        lexer.TokenKind.LBracket() => {
            *pos = idx + 1
            let items: Store<i64> = store_new()
            let first = *store_at(tokens, *pos)
            if is_token(first, lexer.TokenKind.RBracket) {
                *pos = *pos + 1
                return push_node(nodes, JsonValue.Array(JsonArray { items: give items }))
            }
            loop {
                let elem_idx = parse_value(src, tokens, pos, nodes, depth + 1, byte_limit)?
                push_i64(items, elem_idx)?
                let next = *store_at(tokens, *pos)
                if is_token(next, lexer.TokenKind.Comma) {
                    *pos = *pos + 1
                } else if is_token(next, lexer.TokenKind.RBracket) {
                    *pos = *pos + 1
                    break
                } else {
                    return lexer.JsonError.UnexpectedToken
                }
            }
            return push_node(nodes, JsonValue.Array(JsonArray { items: give items }))
        },
        lexer.TokenKind.LBrace() => {
            *pos = idx + 1
            let fields: Store<JsonField> = store_new()
            let first = *store_at(tokens, *pos)
            if is_token(first, lexer.TokenKind.RBrace) {
                *pos = *pos + 1
                return push_node(nodes, JsonValue.Object(JsonObject { fields: give fields }))
            }
            loop {
                let key_tok = *store_at(tokens, *pos)
                if !is_token(key_tok, lexer.TokenKind.String) {
                    return lexer.JsonError.UnexpectedToken
                }
                *pos = *pos + 1
                let key_str = lexer.unescape_string(src, key_tok.start, key_tok.end, byte_limit)?
                let colon_tok = *store_at(tokens, *pos)
                if !is_token(colon_tok, lexer.TokenKind.Colon) {
                    return lexer.JsonError.UnexpectedToken
                }
                *pos = *pos + 1
                let val_idx = parse_value(src, tokens, pos, nodes, depth + 1, byte_limit)?
                push_field(fields, JsonField { key: key_str, value: val_idx })?
                let next = *store_at(tokens, *pos)
                if is_token(next, lexer.TokenKind.Comma) {
                    *pos = *pos + 1
                } else if is_token(next, lexer.TokenKind.RBrace) {
                    *pos = *pos + 1
                    break
                } else {
                    return lexer.JsonError.UnexpectedToken
                }
            }
            return push_node(nodes, JsonValue.Object(JsonObject { fields: give fields }))
        },
        _ => {
            return lexer.JsonError.UnexpectedToken
        }
    }
}

pub fn parse(src: &[u8], byte_limit: i64) -> lexer.JsonError!JsonDocument {
    let tokens = lexer.tokenize(src)?
    let nodes: Store<JsonValue> = store_new()
    var pos: i64 = 0
    let root = parse_value(src, tokens, &mut pos, nodes, 0, byte_limit)?
    let end_tok = *store_at(tokens, pos)
    if !is_token(end_tok, lexer.TokenKind.Eof) {
        return lexer.JsonError.TrailingTokens
    }
    return JsonDocument {
        nodes: give nodes,
        root: root,
    }
}

fn step_query(doc: &JsonDocument, cur: i64, segment: &[u8]) -> lexer.JsonError!i64 {
    if segment.len() == 0 {
        return lexer.JsonError.InvalidQuery
    }
    let node = store_at(doc.nodes, cur)
    match *node {
        JsonValue.Object(ref obj) => {
            var idx: i64 = 0
            let count = store_len(obj.fields)
            while idx < count {
                let field = store_at(obj.fields, idx)
                if lexer.bytes_eq(string.as_bytes(&field.key), segment) {
                    return field.value
                }
                idx += 1
            }
            return lexer.JsonError.KeyNotFound
        },
        JsonValue.Array(ref arr) => {
            let elem_pos = parse_index(segment)?
            if elem_pos < 0 || elem_pos >= store_len(arr.items) {
                return lexer.JsonError.IndexOutOfBounds
            }
            return *store_at(arr.items, elem_pos)
        },
        _ => {
            return lexer.JsonError.InvalidQuery
        }
    }
}

pub fn query_path(doc: &JsonDocument, path: &[u8]) -> lexer.JsonError!i64 {
    if path.len() == 0 || (path.len() == 1 && path[0] == 46u8) {
        return doc.root
    }
    var at: i64 = 0
    if path[0] == 46u8 {
        at = 1
    }
    var cur = doc.root
    while at < path.len() {
        let start = at
        while at < path.len() && path[at] != 46u8 {
            at += 1
        }
        cur = step_query(doc, cur, &path[start..at])?
        if at < path.len() && path[at] == 46u8 {
            at += 1
            if at == path.len() {
                return lexer.JsonError.InvalidQuery
            }
        }
    }
    return cur
}

fn hex_digit(nibble: u8) -> u8 {
    if nibble < 10u8 {
        return 48u8 + nibble
    }
    return 87u8 + nibble
}

fn write_escaped_string(out: ByteBuffer, bytes: &[u8]) -> lexer.JsonError!void {
    lexer.push_byte(out, 34u8)?
    var at: i64 = 0
    while at < bytes.len() {
        let ch = bytes[at]
        if ch == 34u8 {
            lexer.push_slice(out, "\\\"")?
        } else if ch == 92u8 {
            lexer.push_slice(out, "\\\\")?
        } else if ch == 8u8 {
            lexer.push_slice(out, "\\b")?
        } else if ch == 12u8 {
            lexer.push_slice(out, "\\f")?
        } else if ch == 10u8 {
            lexer.push_slice(out, "\\n")?
        } else if ch == 13u8 {
            lexer.push_slice(out, "\\r")?
        } else if ch == 9u8 {
            lexer.push_slice(out, "\\t")?
        } else if ch < 32u8 {
            lexer.push_slice(out, "\\u00")?
            lexer.push_byte(out, hex_digit(ch / 16u8))?
            lexer.push_byte(out, hex_digit(ch % 16u8))?
        } else {
            lexer.push_byte(out, ch)?
        }
        at += 1
    }
    lexer.push_byte(out, 34u8)?
    return ()
}

fn write_node(doc: &JsonDocument, node_idx: i64, out: ByteBuffer, depth: i64) -> lexer.JsonError!void {
    if depth > 64 {
        return lexer.JsonError.DepthLimit
    }
    let node = store_at(doc.nodes, node_idx)
    match *node {
        JsonValue.Null() => {
            lexer.push_slice(out, "null")?
        },
        JsonValue.Bool(b) => {
            if b {
                lexer.push_slice(out, "true")?
            } else {
                lexer.push_slice(out, "false")?
            }
        },
        JsonValue.Int(n) => {
            let dec = int.format_i64(n)
            lexer.push_slice(out, &dec.bytes[0..dec.len])?
        },
        JsonValue.Decimal(ref s) => {
            lexer.push_slice(out, string.as_bytes(s))?
        },
        JsonValue.Str(ref s) => {
            write_escaped_string(out, string.as_bytes(s))?
        },
        JsonValue.Array(ref arr) => {
            lexer.push_byte(out, 91u8)?
            var idx: i64 = 0
            let count = store_len(arr.items)
            while idx < count {
                if idx > 0 {
                    lexer.push_byte(out, 44u8)?
                }
                let child = *store_at(arr.items, idx)
                write_node(doc, child, out, depth + 1)?
                idx += 1
            }
            lexer.push_byte(out, 93u8)?
        },
        JsonValue.Object(ref obj) => {
            lexer.push_byte(out, 123u8)?
            var idx: i64 = 0
            let count = store_len(obj.fields)
            while idx < count {
                if idx > 0 {
                    lexer.push_byte(out, 44u8)?
                }
                let field = store_at(obj.fields, idx)
                write_escaped_string(out, string.as_bytes(&field.key))?
                lexer.push_byte(out, 58u8)?
                write_node(doc, field.value, out, depth + 1)?
                idx += 1
            }
            lexer.push_byte(out, 125u8)?
        },
    }
    return ()
}

pub fn serialize_node(doc: &JsonDocument, node_idx: i64) -> lexer.JsonError!ByteBuffer
    contract { result(create) }
{
    let out = bytes_new()
    write_node(doc, node_idx, out, 0)?
    return give out
}
