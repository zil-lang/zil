import io
resource Handle { fd: i32 }
impl Drop for Handle { fn drop(self: Self) { io.close_fd(.fd) } }
package Pipe { manages input: Handle; manages output: Handle }
fn main() -> i32 {
    let fds = io.open_pipe()
    if fds[0] < 0 { return 10 }
    if true {
        let handles = Pipe { input: Handle { fd: fds[0] }, output: Handle { fd: fds[1] } }
        var buffer = io.empty()
        buffer[0] = 0u8; buffer[1] = 127u8; buffer[2] = 128u8; buffer[3] = 255u8
        buffer[4] = 228u8; buffer[5] = 184u8; buffer[6] = 173u8; buffer[7] = 10u8
        if io.write_all(handles.output.fd, &mut buffer, 8) != 0 { return 11 }
        if true { let finished = give handles.output }
        if !io.closed(fds[1]) || io.closed(fds[0]) { return 12 }
        let count = io.read_chunk(handles.input.fd, &mut buffer)
        if count != 8 { return 13 }
        if io.write_all(1, &mut buffer, count) != 0 { return 14 }
        if io.read_chunk(handles.input.fd, &mut buffer) != 0 { return 15 }
    }
    if !io.closed(fds[0]) || !io.closed(fds[1]) { return 16 }
    return 0
}
