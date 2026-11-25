// Heterogeneous HTTP route dispatch via `trait Handler` and `OwnedDyn<dyn Handler>`.
import protocol
import std.int

pub trait Handler {
    fn handle(self: &Self, req: &protocol.HttpRequest) -> protocol.ProtocolError!protocol.HttpResponse;
}

pub struct HealthHandler {
    pub version: i32
}

impl Handler for HealthHandler {
    fn handle(self: &Self, req: &protocol.HttpRequest) -> protocol.ProtocolError!protocol.HttpResponse {
        let out = bytes_new()
        let prefix = "ok:v"
        var i: i64 = 0
        while i < prefix.len() {
            if !bytes_push(out, prefix[i]) { return protocol.ProtocolError.Allocation }
            i += 1
        }
        let dec = int.format_i32(self.version)
        var j: i64 = 0
        while j < dec.len {
            if !bytes_push(out, dec.bytes[j]) { return protocol.ProtocolError.Allocation }
            j += 1
        }
        return protocol.make_response(200, "OK", bytes_view(out))
    }
}

pub package EchoHandler {
    manages prefix: ByteBuffer
}

impl Handler for EchoHandler {
    fn handle(self: &Self, req: &protocol.HttpRequest) -> protocol.ProtocolError!protocol.HttpResponse {
        match protocol.query_param(protocol.request_query(req), "msg")? {
            Option.None() => {
                return protocol.make_response(400, "Bad Request", "missing msg")
            },
            Option.Some(msg_buf) => {
                let prefix = bytes_view(self.prefix)
                let msg = bytes_view(msg_buf)
                let out = bytes_new()
                if !bytes_reserve(out, prefix.len() + msg.len()) {
                    return protocol.ProtocolError.Allocation
                }
                var i: i64 = 0
                while i < prefix.len() {
                    if !bytes_push(out, prefix[i]) { return protocol.ProtocolError.Allocation }
                    i += 1
                }
                var j: i64 = 0
                while j < msg.len() {
                    if !bytes_push(out, msg[j]) { return protocol.ProtocolError.Allocation }
                    j += 1
                }
                return protocol.make_response(200, "OK", bytes_view(out))
            }
        }
    }
}

pub struct UpperHandler {}

impl Handler for UpperHandler {
    fn handle(self: &Self, req: &protocol.HttpRequest) -> protocol.ProtocolError!protocol.HttpResponse {
        let src = protocol.request_body(req)
        let out = bytes_new()
        if !bytes_reserve(out, src.len()) {
            return protocol.ProtocolError.Allocation
        }
        var i: i64 = 0
        while i < src.len() {
            var ch = src[i]
            if ch >= 97u8 && ch <= 122u8 {
                ch = ch - 32u8
            }
            if !bytes_push(out, ch) { return protocol.ProtocolError.Allocation }
            i += 1
        }
        return protocol.make_response(200, "OK", bytes_view(out))
    }
}

pub struct NotFoundHandler {}

impl Handler for NotFoundHandler {
    fn handle(self: &Self, req: &protocol.HttpRequest) -> protocol.ProtocolError!protocol.HttpResponse {
        return protocol.make_response(404, "Not Found", "not found")
    }
}

fn make_echo_prefix() -> protocol.ProtocolError!ByteBuffer contract { result(create) } {
    let text = "echo:"
    let buf = bytes_new()
    if !bytes_reserve(buf, text.len()) { return protocol.ProtocolError.Allocation }
    var i: i64 = 0
    while i < text.len() {
        if !bytes_push(buf, text[i]) { return protocol.ProtocolError.Allocation }
        i += 1
    }
    return give buf
}

fn select_handler(method: &[u8], path: &[u8]) -> protocol.ProtocolError!OwnedDyn<dyn Handler> {
    if protocol.bytes_eq(method, "GET") && protocol.bytes_eq(path, "/health") {
        match owned_dyn::<dyn Handler>(HealthHandler { version: 1 }) {
            .ok(h) => { return h },
            .err(_) => { return protocol.ProtocolError.Allocation }
        }
    }
    if protocol.bytes_eq(method, "GET") && protocol.bytes_eq(path, "/echo") {
        let prefix = make_echo_prefix()?
        match owned_dyn::<dyn Handler>(EchoHandler { prefix: give prefix }) {
            .ok(h) => { return h },
            .err(_) => { return protocol.ProtocolError.Allocation }
        }
    }
    if protocol.bytes_eq(method, "POST") && protocol.bytes_eq(path, "/upper") {
        match owned_dyn::<dyn Handler>(UpperHandler {}) {
            .ok(h) => { return h },
            .err(_) => { return protocol.ProtocolError.Allocation }
        }
    }
    match owned_dyn::<dyn Handler>(NotFoundHandler {}) {
        .ok(h) => { return h },
        .err(_) => { return protocol.ProtocolError.Allocation }
    }
}

pub fn dispatch(req: &protocol.HttpRequest) -> protocol.ProtocolError!protocol.HttpResponse {
    let handler = select_handler(protocol.request_method(req), protocol.request_path(req))?
    return handler.handle(req)
}
