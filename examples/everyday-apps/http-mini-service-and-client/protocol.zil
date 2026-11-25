// HTTP/1.1 wire-format request and response parser and serializer.
import std.int

pub enum ProtocolError {
    MalformedRequest,
    MalformedResponse,
    UnsupportedVersion,
    BodyTooLarge,
    Allocation
}

pub package HttpRequest {
    manages method: ByteBuffer;
    manages target: ByteBuffer;
    manages path: ByteBuffer;
    manages query: ByteBuffer;
    manages body: ByteBuffer
}

pub package HttpResponse {
    pub status: i32;
    manages reason: ByteBuffer;
    manages body: ByteBuffer
}

pub fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false }
    var i: i64 = 0
    while i < a.len() {
        if a[i] != b[i] { return false }
        i += 1
    }
    return true
}

fn ascii_lower(b: u8) -> u8 {
    if b >= 65u8 && b <= 90u8 {
        return b + 32u8
    }
    return b
}

fn bytes_eq_ascii_icase(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false }
    var i: i64 = 0
    while i < a.len() {
        if ascii_lower(a[i]) != ascii_lower(b[i]) { return false }
        i += 1
    }
    return true
}

fn append_bytes(buf: ByteBuffer, data: &[u8]) -> ProtocolError!void {
    let cur = bytes_len(buf)
    let needed = cur + data.len()
    if needed > bytes_capacity(buf) {
        var next = bytes_capacity(buf)
        if next < 64 { next = 64 }
        while next < needed {
            next *= 2
        }
        if !bytes_reserve(buf, next) { return ProtocolError.Allocation }
    }
    var i: i64 = 0
    while i < data.len() {
        if !bytes_push(buf, data[i]) { return ProtocolError.Allocation }
        i += 1
    }
    return ()
}

fn copy_slice(data: &[u8]) -> ProtocolError!ByteBuffer contract { result(create) } {
    let out = bytes_new()
    append_bytes(out, data)?
    return give out
}

fn find_byte(hay: &[u8], start: i64, needle: u8) -> i64 {
    var i = start
    while i < hay.len() {
        if hay[i] == needle { return i }
        i += 1
    }
    return -1
}

fn find_crlf(hay: &[u8], start: i64) -> i64 {
    var i = start
    while i + 1 < hay.len() {
        if hay[i] == 13u8 && hay[i + 1] == 10u8 { return i }
        i += 1
    }
    return -1
}

fn find_header_end(hay: &[u8]) -> i64 {
    var i: i64 = 0
    while i + 3 < hay.len() {
        if hay[i] == 13u8 && hay[i + 1] == 10u8 && hay[i + 2] == 13u8 && hay[i + 3] == 10u8 {
            return i
        }
        i += 1
    }
    return -1
}

fn trim_ows(data: &[u8]) -> &[u8] {
    var start: i64 = 0
    var end = data.len()
    while start < end && (data[start] == 32u8 || data[start] == 9u8) {
        start += 1
    }
    while end > start && (data[end - 1] == 32u8 || data[end - 1] == 9u8) {
        end -= 1
    }
    return &data[start..end]
}

fn parse_content_length_header(headers_block: &[u8]) -> ProtocolError!Option<i64> {
    var pos: i64 = 0
    while pos < headers_block.len() {
        var line_end = find_crlf(headers_block, pos)
        if line_end < 0 { line_end = headers_block.len() }
        let line = &headers_block[pos..line_end]
        if line.len() > 0 {
            let colon = find_byte(line, 0, 58u8)
            if colon <= 0 { return ProtocolError.MalformedRequest }
            let name = trim_ows(&line[0..colon])
            let val = trim_ows(&line[colon + 1..line.len()])
            if bytes_eq_ascii_icase(name, "content-length") {
                match int.parse_i64(val) {
                    .err(_) => { return ProtocolError.MalformedRequest },
                    .ok(n) => {
                        if n < 0 { return ProtocolError.MalformedRequest }
                        return Option::<i64>.Some(n)
                    }
                }
            }
        }
        if line_end + 2 <= headers_block.len() {
            pos = line_end + 2
        } else {
            break
        }
    }
    return Option::<i64>.None()
}

pub fn make_response(status: i32, reason: &[u8], body: &[u8]) -> ProtocolError!HttpResponse {
    let reason_buf = copy_slice(reason)?
    let body_buf = copy_slice(body)?
    return HttpResponse {
        status: status,
        reason: give reason_buf,
        body: give body_buf
    }
}

pub fn request_method(req: &HttpRequest) -> &[u8] {
    return bytes_view(req.method)
}

pub fn request_target(req: &HttpRequest) -> &[u8] {
    return bytes_view(req.target)
}

pub fn request_path(req: &HttpRequest) -> &[u8] {
    return bytes_view(req.path)
}

pub fn request_query(req: &HttpRequest) -> &[u8] {
    return bytes_view(req.query)
}

pub fn request_body(req: &HttpRequest) -> &[u8] {
    return bytes_view(req.body)
}

pub fn response_reason(resp: &HttpResponse) -> &[u8] {
    return bytes_view(resp.reason)
}

pub fn response_body(resp: &HttpResponse) -> &[u8] {
    return bytes_view(resp.body)
}

pub fn parse_request(raw: &[u8]) -> ProtocolError!HttpRequest {
    let header_end = find_header_end(raw)
    if header_end < 0 { return ProtocolError.MalformedRequest }
    let first_crlf = find_crlf(raw, 0)
    if first_crlf <= 0 || first_crlf > header_end { return ProtocolError.MalformedRequest }

    let request_line = &raw[0..first_crlf]
    let sp_one = find_byte(request_line, 0, 32u8)
    if sp_one <= 0 { return ProtocolError.MalformedRequest }
    let sp_two = find_byte(request_line, sp_one + 1, 32u8)
    if sp_two <= sp_one + 1 || sp_two + 1 >= request_line.len() {
        return ProtocolError.MalformedRequest
    }
    if find_byte(request_line, sp_two + 1, 32u8) >= 0 {
        return ProtocolError.MalformedRequest
    }

    let method_slice = &request_line[0..sp_one]
    let target_slice = &request_line[sp_one + 1..sp_two]
    let version_slice = &request_line[sp_two + 1..request_line.len()]
    if !bytes_eq(version_slice, "HTTP/1.1") {
        return ProtocolError.UnsupportedVersion
    }

    let body_start = header_end + 4
    let body_slice = &raw[body_start..raw.len()]
    if first_crlf + 2 <= header_end {
        let headers_slice = &raw[first_crlf + 2..header_end]
        match parse_content_length_header(headers_slice)? {
            Option.None() => {},
            Option.Some(expected_len) => {
                if expected_len > 65536 { return ProtocolError.BodyTooLarge }
                if body_slice.len() != expected_len {
                    return ProtocolError.MalformedRequest
                }
            }
        }
    }

    let qmark = find_byte(target_slice, 0, 63u8)
    var path_slice = target_slice
    var query_slice = &target_slice[0..0]
    if qmark >= 0 {
        path_slice = &target_slice[0..qmark]
        query_slice = &target_slice[qmark + 1..target_slice.len()]
    }

    let method_buf = copy_slice(method_slice)?
    let target_buf = copy_slice(target_slice)?
    let path_buf = copy_slice(path_slice)?
    let query_buf = copy_slice(query_slice)?
    let body_buf = copy_slice(body_slice)?
    return HttpRequest {
        method: give method_buf,
        target: give target_buf,
        path: give path_buf,
        query: give query_buf,
        body: give body_buf
    }
}

pub fn format_request(method: &[u8], target: &[u8], body: &[u8]) -> ProtocolError!ByteBuffer contract { result(create) } {
    let out = bytes_new()
    append_bytes(out, method)?
    append_bytes(out, " ")?
    append_bytes(out, target)?
    append_bytes(out, " HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: ")?
    let len_dec = int.format_i64(body.len())
    append_bytes(out, &len_dec.bytes[0..len_dec.len])?
    append_bytes(out, "\r\n\r\n")?
    append_bytes(out, body)?
    return give out
}

pub fn format_response(resp: &HttpResponse) -> ProtocolError!ByteBuffer contract { result(create) } {
    let out = bytes_new()
    append_bytes(out, "HTTP/1.1 ")?
    let status_dec = int.format_i32(resp.status)
    append_bytes(out, &status_dec.bytes[0..status_dec.len])?
    append_bytes(out, " ")?
    append_bytes(out, bytes_view(resp.reason))?
    append_bytes(out, "\r\nContent-Type: text/plain\r\nContent-Length: ")?
    let body_view = bytes_view(resp.body)
    let len_dec = int.format_i64(body_view.len())
    append_bytes(out, &len_dec.bytes[0..len_dec.len])?
    append_bytes(out, "\r\nConnection: close\r\n\r\n")?
    append_bytes(out, body_view)?
    return give out
}

pub fn parse_response(raw: &[u8]) -> ProtocolError!HttpResponse {
    let header_end = find_header_end(raw)
    if header_end < 0 { return ProtocolError.MalformedResponse }
    let first_crlf = find_crlf(raw, 0)
    if first_crlf <= 0 || first_crlf > header_end { return ProtocolError.MalformedResponse }

    let status_line = &raw[0..first_crlf]
    let sp_one = find_byte(status_line, 0, 32u8)
    if sp_one <= 0 { return ProtocolError.MalformedResponse }
    let sp_two = find_byte(status_line, sp_one + 1, 32u8)
    if sp_two <= sp_one + 1 || sp_two + 1 >= status_line.len() {
        return ProtocolError.MalformedResponse
    }

    let version_slice = &status_line[0..sp_one]
    if !bytes_eq(version_slice, "HTTP/1.1") {
        return ProtocolError.UnsupportedVersion
    }
    let code_slice = &status_line[sp_one + 1..sp_two]
    let reason_slice = &status_line[sp_two + 1..status_line.len()]
    var status: i32 = 0
    match int.parse_i32(code_slice) {
        .err(_) => { return ProtocolError.MalformedResponse },
        .ok(code) => { status = code }
    }

    let body_start = header_end + 4
    let body_slice = &raw[body_start..raw.len()]
    if first_crlf + 2 > header_end {
        return ProtocolError.MalformedResponse
    }
    let headers_slice = &raw[first_crlf + 2..header_end]
    match parse_content_length_header(headers_slice) {
        .err(_) => { return ProtocolError.MalformedResponse },
        .ok(maybe_len) => {
            match maybe_len {
                Option.None() => { return ProtocolError.MalformedResponse },
                Option.Some(expected_len) => {
                    if body_slice.len() != expected_len {
                        return ProtocolError.MalformedResponse
                    }
                }
            }
        }
    }

    let reason_buf = copy_slice(reason_slice)?
    let body_buf = copy_slice(body_slice)?
    return HttpResponse {
        status: status,
        reason: give reason_buf,
        body: give body_buf
    }
}

pub fn query_param(query: &[u8], key: &[u8]) -> ProtocolError!Option<Managed<ByteBuffer>> {
    var pos: i64 = 0
    while pos < query.len() {
        var amp = find_byte(query, pos, 38u8)
        if amp < 0 { amp = query.len() }
        let pair = &query[pos..amp]
        let eq = find_byte(pair, 0, 61u8)
        if eq >= 0 {
            let k = &pair[0..eq]
            let v = &pair[eq + 1..pair.len()]
            if bytes_eq(k, key) {
                let copied = copy_slice(v)?
                return Option::<Managed<ByteBuffer>>.Some(give copied)
            }
        }
        pos = amp + 1
    }
    return Option::<Managed<ByteBuffer>>.None()
}
