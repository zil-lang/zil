// Everyday App: HTTP/1.1 mini service and client over loopback POSIX TCP.
// Spawns a real server worker via `thread_scope` synchronized via `AtomicI32`
// Acquire/Release port publication, dispatches routes through
// `OwnedDyn<dyn router.Handler>`, verifies HTTP/1.1 status/headers/body from a
// real `TcpStream` client, and enforces deterministic socket teardown.
import protocol
import router
import std.io
import std.net
import std.time

fn handle_one_connection(give conn: net.TcpStream) -> i32 {
    var stream = give conn
    match stream.read_to_end(65536) {
        .err(_) => { return 31 },
        .ok(raw_req) => {
            var wire_resp = bytes_new()
            match protocol.parse_request(bytes_view(raw_req)) {
                .err(_) => {
                    match protocol.make_response(400, "Bad Request", "bad request") {
                        .err(_) => { return 32 },
                        .ok(bad_resp) => {
                            match protocol.format_response(&bad_resp) {
                                .err(_) => { return 33 },
                                .ok(formatted) => { wire_resp = give formatted }
                            }
                        }
                    }
                },
                .ok(req) => {
                    match router.dispatch(&req) {
                        .err(_) => { return 34 },
                        .ok(resp) => {
                            match protocol.format_response(&resp) {
                                .err(_) => { return 35 },
                                .ok(formatted) => { wire_resp = give formatted }
                            }
                        }
                    }
                }
            }
            match stream.write_all(bytes_view(wire_resp)) {
                .err(_) => { return 36 },
                .ok(_) => {}
            }
            match stream.shutdown_write() {
                .err(_) => { return 37 },
                .ok(_) => {}
            }
            match stream.close() {
                .err(_) => { return 38 },
                .ok(_) => {}
            }
            return 0
        }
    }
}

fn server_worker(port_cell: &AtomicI32) -> i32 {
    match net.TcpListener.bind(net.localhost(), 0) {
        .err(_) => {
            atomic_store_ordered(port_cell, -1, MemoryOrder.Release)
            return 28
        },
        .ok(bound) => {
            var listener = give bound
            var port: i32 = 0
            match listener.local_port() {
                .err(_) => {
                    atomic_store_ordered(port_cell, -2, MemoryOrder.Release)
                    return 29
                },
                .ok(p) => { port = p }
            }
            if port <= 0 || port > 65535 {
                atomic_store_ordered(port_cell, -3, MemoryOrder.Release)
                return 29
            }
            // Publish bound ephemeral port to client with Release ordering.
            atomic_store_ordered(port_cell, port, MemoryOrder.Release)

            let expected_requests: i32 = 11
            var handled: i32 = 0
            while handled < expected_requests {
                match listener.accept() {
                    .err(_) => { return 30 },
                    .ok(accepted) => {
                        let rc = handle_one_connection(give accepted)
                        if rc != 0 { return rc }
                        handled += 1
                    }
                }
            }
            match listener.close() {
                .err(_) => { return 39 },
                .ok(_) => { return 0 }
            }
        }
    }
}

fn send_raw_and_verify(
    port: i32,
    label: &[u8],
    raw_request: &[u8],
    want_status: i32,
    want_reason: &[u8],
    want_body: &[u8]
) -> i32 {
    match net.TcpStream.connect(net.localhost(), port) {
        .err(_) => { return 40 },
        .ok(conn) => {
            var stream = give conn
            match stream.write_all(raw_request) {
                .err(_) => { return 41 },
                .ok(_) => {}
            }
            match stream.shutdown_write() {
                .err(_) => { return 42 },
                .ok(_) => {}
            }
            match stream.read_to_end(65536) {
                .err(_) => { return 43 },
                .ok(raw_resp) => {
                    match stream.close() {
                        .err(_) => { return 44 },
                        .ok(_) => {}
                    }
                    match protocol.parse_response(bytes_view(raw_resp)) {
                        .err(_) => { return 45 },
                        .ok(resp) => {
                            if resp.status != want_status { return 46 }
                            if !protocol.bytes_eq(protocol.response_reason(&resp), want_reason) {
                                return 47
                            }
                            if !protocol.bytes_eq(protocol.response_body(&resp), want_body) {
                                return 48
                            }
                            match io.print(label) { .ok(_) => {}, .err(_) => { return 49 } }
                            match io.print(" -> ") { .ok(_) => {}, .err(_) => { return 49 } }
                            match io.print_i64(i64(resp.status)) { .ok(_) => {}, .err(_) => { return 49 } }
                            match io.print(" ") { .ok(_) => {}, .err(_) => { return 49 } }
                            match io.println(protocol.response_body(&resp)) { .ok(_) => {}, .err(_) => { return 49 } }
                            return 0
                        }
                    }
                }
            }
        }
    }
}

fn send_http_and_verify(
    port: i32,
    label: &[u8],
    method: &[u8],
    target: &[u8],
    body: &[u8],
    want_status: i32,
    want_reason: &[u8],
    want_body: &[u8]
) -> i32 {
    match protocol.format_request(method, target, body) {
        .err(_) => { return 39 },
        .ok(req_bytes) => {
            return send_raw_and_verify(
                port,
                label,
                bytes_view(req_bytes),
                want_status,
                want_reason,
                want_body
            )
        }
    }
}

fn wait_for_port(port_cell: &AtomicI32) -> i32 {
    var spins: i32 = 0
    while spins < 5000 {
        let p = atomic_load_ordered(port_cell, MemoryOrder.Acquire)
        if p != 0 { return p }
        time.sleep_ms(1)
        spins += 1
    }
    return -4
}

fn run_client_suite(port: i32) -> i32 {
    let r_health = send_http_and_verify(
        port,
        "GET /health",
        "GET",
        "/health",
        "",
        200,
        "OK",
        "ok:v1"
    )
    if r_health != 0 { return r_health }

    let r_echo = send_http_and_verify(
        port,
        "GET /echo?msg=zil-alpha",
        "GET",
        "/echo?msg=zil-alpha",
        "",
        200,
        "OK",
        "echo:zil-alpha"
    )
    if r_echo != 0 { return r_echo }

    let r_echo_empty = send_http_and_verify(
        port,
        "GET /echo?a=1&msg=&b=2",
        "GET",
        "/echo?a=1&msg=&b=2",
        "",
        200,
        "OK",
        "echo:"
    )
    if r_echo_empty != 0 { return r_echo_empty }

    let r_echo_missing = send_http_and_verify(
        port,
        "GET /echo",
        "GET",
        "/echo",
        "",
        400,
        "Bad Request",
        "missing msg"
    )
    if r_echo_missing != 0 { return r_echo_missing }

    let r_upper = send_http_and_verify(
        port,
        "POST /upper",
        "POST",
        "/upper",
        "hello zil 42",
        200,
        "OK",
        "HELLO ZIL 42"
    )
    if r_upper != 0 { return r_upper }

    let r_missing = send_http_and_verify(
        port,
        "GET /missing",
        "GET",
        "/missing",
        "",
        404,
        "Not Found",
        "not found"
    )
    if r_missing != 0 { return r_missing }

    let r_bad = send_raw_and_verify(
        port,
        "BAD",
        "GARBAGE\r\n\r\n",
        400,
        "Bad Request",
        "bad request"
    )
    if r_bad != 0 { return r_bad }

    let r_bad_cl = send_raw_and_verify(
        port,
        "BAD_CL",
        "POST /upper HTTP/1.1\r\nHost: localhost\r\nContent-Length: 10\r\n\r\nshort",
        400,
        "Bad Request",
        "bad request"
    )
    if r_bad_cl != 0 { return r_bad_cl }

    let r_bad_dup_cl = send_raw_and_verify(
        port,
        "BAD_DUP_CL",
        "POST /upper HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\nhi",
        400,
        "Bad Request",
        "bad request"
    )
    if r_bad_dup_cl != 0 { return r_bad_dup_cl }

    let r_bad_ver = send_raw_and_verify(
        port,
        "BAD_VER",
        "GET /health HTTP/1.0\r\nHost: localhost\r\n\r\n",
        400,
        "Bad Request",
        "bad request"
    )
    if r_bad_ver != 0 { return r_bad_ver }

    let r_bad_hdr = send_raw_and_verify(
        port,
        "BAD_HDR",
        "GET /health HTTP/1.1\r\nBrokenHeaderWithoutColon\r\n\r\n",
        400,
        "Bad Request",
        "bad request"
    )
    if r_bad_hdr != 0 { return r_bad_hdr }

    return 0
}

fn main() -> i32 {
    let (ok, rc) = atomic_scope(0, (port_cell: &AtomicI32) -> i32 => {
        let (created, server_rc, client_rc) = thread_scope(
            server_worker,
            port_cell,
            () -> i32 => {
                let port = wait_for_port(port_cell)
                if port <= 0 { return 10 }
                return run_client_suite(port)
            }
        )
        if !created { return 5 }
        if server_rc != 0 { return server_rc }
        if client_rc != 0 { return client_rc }

        // Verify deterministic teardown: listener is already closed by server_worker.
        let closed_port = atomic_load_ordered(port_cell, MemoryOrder.Acquire)
        if closed_port <= 0 { return 6 }
        match net.TcpStream.connect(net.localhost(), closed_port) {
            .ok(_) => { return 7 },
            .err(_) => {}
        }
        return 0
    })
    if !ok { return 1 }
    if rc != 0 { return rc }

    match io.print("SERVED ") { .ok(_) => {}, .err(_) => { return 8 } }
    match io.println_i64(11) { .ok(_) => {}, .err(_) => { return 8 } }
    return 0
}
