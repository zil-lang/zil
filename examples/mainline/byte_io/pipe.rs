mod io;
struct Handle {
    fd: i32,
}
impl Drop for Handle {
    fn drop(&mut self) {
        io::close_fd(self.fd);
    }
}
struct Pipe {
    input: Handle,
    output: Option<Handle>,
}
fn application() -> i32 {
    let fds = io::open_pipe();
    if fds[0] < 0 {
        return 10;
    }
    {
        let mut handles = Pipe {
            input: Handle { fd: fds[0] },
            output: Some(Handle { fd: fds[1] }),
        };
        let mut buffer = io::empty();
        buffer[..8].copy_from_slice(&[0, 127, 128, 255, 228, 184, 173, 10]);
        if io::write_all(handles.output.as_ref().unwrap().fd, &mut buffer, 8) != 0 {
            return 11;
        }
        {
            let _finished = handles.output.take().unwrap();
        }
        if !io::closed(fds[1]) || io::closed(fds[0]) {
            return 12;
        }
        let count = io::read_chunk(handles.input.fd, &mut buffer);
        if count != 8 {
            return 13;
        }
        if io::write_all(1, &mut buffer, count) != 0 {
            return 14;
        }
        if io::read_chunk(handles.input.fd, &mut buffer) != 0 {
            return 15;
        }
    }
    if !io::closed(fds[0]) || !io::closed(fds[1]) {
        return 16;
    }
    0
}
fn main() {
    std::process::exit(application());
}
