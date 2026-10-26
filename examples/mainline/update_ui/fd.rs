unsafe extern "C" {
    fn dup(fd: i32) -> i32;
    fn close(fd: i32) -> i32;
}
struct Handle {
    fd: i32,
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            close(self.fd);
        }
    }
}
struct Pair {
    first: Option<Handle>,
    second: Option<Handle>,
}
fn duplicate(fd: i32) -> i32 {
    unsafe { dup(fd) }
}
fn closed(fd: i32) -> bool {
    let probe = duplicate(fd);
    if probe >= 0 {
        unsafe {
            close(probe);
        }
        false
    } else {
        true
    }
}
fn run() -> u8 {
    let first = duplicate(0);
    if first < 0 {
        return 10;
    }
    let second = duplicate(0);
    if second < 0 {
        unsafe {
            close(first);
        }
        return 11;
    }
    let third = duplicate(0);
    if third < 0 {
        unsafe {
            close(first);
            close(second);
        }
        return 12;
    }
    {
        let mut pair = Pair {
            first: Some(Handle { fd: first }),
            second: Some(Handle { fd: second }),
        };
        pair.first = Some(Handle { fd: third });
        if !closed(first) {
            return 13;
        }
        {
            let _removed = pair.second.take().unwrap();
        }
        if !closed(second) {
            return 14;
        }
        if pair.first.as_ref().unwrap().fd != third {
            return 15;
        }
    }
    if !closed(third) {
        return 16;
    }
    print!("F");
    0
}
fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(run())
}
