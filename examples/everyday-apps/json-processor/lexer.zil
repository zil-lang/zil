// RFC 8259 JSON lexer and string unescaper.
import std.store
import std.string
import std.utf8

pub enum JsonError {
    UnexpectedChar,
    UnterminatedString,
    InvalidEscape,
    InvalidNumber,
    UnexpectedToken,
    TrailingTokens,
    DepthLimit,
    KeyNotFound,
    IndexOutOfBounds,
    InvalidQuery,
    InvalidUtf8,
    Allocation,
    Io,
}

pub enum TokenKind {
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Colon,
    Comma,
    True,
    False,
    Null,
    String,
    Number,
    Eof,
}

pub struct Token {
    pub kind: TokenKind;
    pub start: i64;
    pub end: i64;
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

pub fn push_byte(buf: ByteBuffer, byte: u8) -> JsonError!void {
    let cur = bytes_len(buf)
    if cur >= bytes_capacity(buf) {
        var next: i64 = 32
        if cur >= 32 {
            if cur > 4611686018427387903 {
                return JsonError.Allocation
            }
            next = cur * 2
        }
        if !bytes_reserve(buf, next) {
            return JsonError.Allocation
        }
    }
    if !bytes_push(buf, byte) {
        return JsonError.Allocation
    }
    return ()
}

pub fn push_slice(buf: ByteBuffer, slice: &[u8]) -> JsonError!void {
    var at: i64 = 0
    while at < slice.len() {
        push_byte(buf, slice[at])?
        at += 1
    }
    return ()
}

fn is_ws(ch: u8) -> bool {
    return ch == 32u8 || ch == 9u8 || ch == 10u8 || ch == 13u8
}

fn is_digit(ch: u8) -> bool {
    return ch >= 48u8 && ch <= 57u8
}

fn hex_value(ch: u8) -> i32 {
    if ch >= 48u8 && ch <= 57u8 {
        return i32(ch) - 48
    }
    if ch >= 65u8 && ch <= 70u8 {
        return i32(ch) - 55
    }
    if ch >= 97u8 && ch <= 102u8 {
        return i32(ch) - 87
    }
    return -1
}

fn is_ident_tail(ch: u8) -> bool {
    if ch >= 48u8 && ch <= 57u8 { return true }
    if ch >= 65u8 && ch <= 90u8 { return true }
    if ch >= 97u8 && ch <= 122u8 { return true }
    return ch == 95u8
}

fn match_keyword(src: &[u8], pos: i64, kw: &[u8]) -> bool {
    if pos + kw.len() > src.len() {
        return false
    }
    var i: i64 = 0
    while i < kw.len() {
        if src[pos + i] != kw[i] {
            return false
        }
        i += 1
    }
    if pos + kw.len() < src.len() && is_ident_tail(src[pos + kw.len()]) {
        return false
    }
    return true
}

fn push_token(tokens: Store<Token>, tok: Token) -> JsonError!void {
    match store.push_growing(tokens, tok) {
        .ok(_) => { return () },
        .err(_) => { return JsonError.Allocation }
    }
}

fn is_simple_escape(esc: u8) -> bool {
    if esc == 34u8 || esc == 92u8 || esc == 47u8 { return true }
    if esc == 98u8 || esc == 102u8 || esc == 110u8 { return true }
    return esc == 114u8 || esc == 116u8
}

fn parse_hex4(src: &[u8], pos: i64, limit: i64) -> i32 {
    if pos + 4 > limit {
        return -1
    }
    let h0 = hex_value(src[pos])
    let h1 = hex_value(src[pos + 1])
    let h2 = hex_value(src[pos + 2])
    let h3 = hex_value(src[pos + 3])
    if h0 < 0 || h1 < 0 || h2 < 0 || h3 < 0 {
        return -1
    }
    return ((h0 * 16 + h1) * 16 + h2) * 16 + h3
}

fn scan_string(src: &[u8], start: i64) -> JsonError!i64 {
    var at = start + 1
    while at < src.len() {
        let ch = src[at]
        if ch == 34u8 {
            return at + 1
        }
        if ch < 32u8 {
            return JsonError.UnexpectedChar
        }
        if ch == 92u8 {
            at += 1
            if at >= src.len() {
                return JsonError.UnterminatedString
            }
            let esc = src[at]
            if is_simple_escape(esc) {
                at += 1
            } else if esc == 117u8 {
                let u1 = parse_hex4(src, at + 1, src.len())
                if u1 < 0 {
                    return JsonError.InvalidEscape
                }
                if u1 >= 55296 && u1 <= 56319 {
                    if at + 10 >= src.len() || src[at + 5] != 92u8 || src[at + 6] != 117u8 {
                        return JsonError.InvalidEscape
                    }
                    let u2 = parse_hex4(src, at + 7, src.len())
                    if u2 < 56320 || u2 > 57343 {
                        return JsonError.InvalidEscape
                    }
                    at += 11
                } else if u1 >= 56320 && u1 <= 57343 {
                    return JsonError.InvalidEscape
                } else {
                    at += 5
                }
            } else {
                return JsonError.InvalidEscape
            }
        } else {
            at += 1
        }
    }
    return JsonError.UnterminatedString
}

fn scan_number(src: &[u8], start: i64) -> JsonError!i64 {
    var at = start
    if at < src.len() && src[at] == 45u8 {
        at += 1
    }
    if at >= src.len() || !is_digit(src[at]) {
        return JsonError.InvalidNumber
    }
    if src[at] == 48u8 {
        at += 1
        if at < src.len() && is_digit(src[at]) {
            return JsonError.InvalidNumber
        }
    } else {
        while at < src.len() && is_digit(src[at]) {
            at += 1
        }
    }
    if at < src.len() && src[at] == 46u8 {
        at += 1
        if at >= src.len() || !is_digit(src[at]) {
            return JsonError.InvalidNumber
        }
        while at < src.len() && is_digit(src[at]) {
            at += 1
        }
    }
    if at < src.len() && (src[at] == 101u8 || src[at] == 69u8) {
        at += 1
        if at < src.len() && (src[at] == 43u8 || src[at] == 45u8) {
            at += 1
        }
        if at >= src.len() || !is_digit(src[at]) {
            return JsonError.InvalidNumber
        }
        while at < src.len() && is_digit(src[at]) {
            at += 1
        }
    }
    if at < src.len() && is_ident_tail(src[at]) {
        return JsonError.InvalidNumber
    }
    return at
}

pub fn tokenize(src: &[u8]) -> JsonError!Store<Token>
    contract { result(create) }
{
    let tokens: Store<Token> = store_new()
    var at: i64 = 0
    while at < src.len() {
        let ch = src[at]
        if is_ws(ch) {
            at += 1
        } else if ch == 123u8 {
            push_token(tokens, Token { kind: TokenKind.LBrace, start: at, end: at + 1 })?
            at += 1
        } else if ch == 125u8 {
            push_token(tokens, Token { kind: TokenKind.RBrace, start: at, end: at + 1 })?
            at += 1
        } else if ch == 91u8 {
            push_token(tokens, Token { kind: TokenKind.LBracket, start: at, end: at + 1 })?
            at += 1
        } else if ch == 93u8 {
            push_token(tokens, Token { kind: TokenKind.RBracket, start: at, end: at + 1 })?
            at += 1
        } else if ch == 58u8 {
            push_token(tokens, Token { kind: TokenKind.Colon, start: at, end: at + 1 })?
            at += 1
        } else if ch == 44u8 {
            push_token(tokens, Token { kind: TokenKind.Comma, start: at, end: at + 1 })?
            at += 1
        } else if ch == 34u8 {
            let end = scan_string(src, at)?
            push_token(tokens, Token { kind: TokenKind.String, start: at, end: end })?
            at = end
        } else if ch == 45u8 || is_digit(ch) {
            let end = scan_number(src, at)?
            push_token(tokens, Token { kind: TokenKind.Number, start: at, end: end })?
            at = end
        } else if match_keyword(src, at, "true") {
            push_token(tokens, Token { kind: TokenKind.True, start: at, end: at + 4 })?
            at += 4
        } else if match_keyword(src, at, "false") {
            push_token(tokens, Token { kind: TokenKind.False, start: at, end: at + 5 })?
            at += 5
        } else if match_keyword(src, at, "null") {
            push_token(tokens, Token { kind: TokenKind.Null, start: at, end: at + 4 })?
            at += 4
        } else {
            return JsonError.UnexpectedChar
        }
    }
    push_token(tokens, Token { kind: TokenKind.Eof, start: src.len(), end: src.len() })?
    return give tokens
}

pub fn unescape_string(src: &[u8], start: i64, end: i64, byte_limit: i64) -> JsonError!string.String {
    if end - start < 2 || src[start] != 34u8 || src[end - 1] != 34u8 {
        return JsonError.UnterminatedString
    }
    let out = bytes_new()
    var at = start + 1
    let inner_end = end - 1
    while at < inner_end {
        let ch = src[at]
        if ch == 92u8 {
            at += 1
            let esc = src[at]
            if esc == 34u8 {
                push_byte(out, 34u8)?
                at += 1
            } else if esc == 92u8 {
                push_byte(out, 92u8)?
                at += 1
            } else if esc == 47u8 {
                push_byte(out, 47u8)?
                at += 1
            } else if esc == 98u8 {
                push_byte(out, 8u8)?
                at += 1
            } else if esc == 102u8 {
                push_byte(out, 12u8)?
                at += 1
            } else if esc == 110u8 {
                push_byte(out, 10u8)?
                at += 1
            } else if esc == 114u8 {
                push_byte(out, 13u8)?
                at += 1
            } else if esc == 116u8 {
                push_byte(out, 9u8)?
                at += 1
            } else if esc == 117u8 {
                let u1 = parse_hex4(src, at + 1, inner_end)
                if u1 < 0 {
                    return JsonError.InvalidEscape
                }
                var scalar = u1
                var step: i64 = 5
                if u1 >= 55296 && u1 <= 56319 {
                    if at + 10 >= inner_end || src[at + 5] != 92u8 || src[at + 6] != 117u8 {
                        return JsonError.InvalidEscape
                    }
                    let u2 = parse_hex4(src, at + 7, inner_end)
                    if u2 < 56320 || u2 > 57343 {
                        return JsonError.InvalidEscape
                    }
                    scalar = 65536 + (u1 - 55296) * 1024 + (u2 - 56320)
                    step = 11
                } else if u1 >= 56320 && u1 <= 57343 {
                    return JsonError.InvalidEscape
                }
                var enc = [0u8, 0u8, 0u8, 0u8]
                let width = utf8.encode(scalar, &mut enc[..])
                if width <= 0 {
                    return JsonError.InvalidEscape
                }
                push_slice(out, &enc[0..width])?
                at += step
            } else {
                return JsonError.InvalidEscape
            }
        } else {
            push_byte(out, ch)?
            at += 1
        }
    }
    match string.from_utf8(bytes_view(out), byte_limit) {
        .ok(s) => { return s },
        .err(_) => { return JsonError.InvalidUtf8 }
    }
}
