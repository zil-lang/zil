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
    first: Handle,
    second: Handle,
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
fn keeper(mut pair: Pair) -> impl FnMut(i32) -> i32 {
    move |replacement| {
        let pair = &mut pair;
        if replacement >= 0 {
            pair.first = Handle { fd: replacement };
        }
        pair.second.fd
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
        let mut kept = keeper(Pair {
            first: Handle { fd: first },
            second: Handle { fd: second },
        });
        if kept(-1) != second || closed(first) || closed(second) {
            return 13;
        }
        if kept(third) != second {
            return 14;
        }
        if !closed(first) || closed(second) || closed(third) {
            return 15;
        }
        if kept(-1) != second {
            return 16;
        }
    }
    if !closed(second) || !closed(third) {
        return 17;
    }
    print!("F");
    0
}
fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(run())
}
