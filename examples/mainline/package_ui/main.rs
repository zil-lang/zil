//! Independent bounded event-loop/lifetime oracle; no ZIL IR is consumed.
use std::io::{self, Read, Write};
fn emit(bytes: &[u8]) {
    io::stdout().write_all(bytes).unwrap();
}
struct Model {
    count: i32,
    frames: i32,
    dirty: bool,
}
struct Surface {
    width: i32,
    height: i32,
}
struct Subscription {
    active: bool,
}
impl Drop for Model {
    fn drop(&mut self) {
        emit(b"M");
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        emit(b"S");
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        emit(b"U");
    }
}
struct Window {
    model: Model,
    surface: Surface,
    subscription: Subscription,
    identity: i32,
}
fn create() -> Window {
    Window {
        surface: Surface {
            width: 80,
            height: 24,
        },
        model: Model {
            count: 0,
            frames: 0,
            dirty: true,
        },
        subscription: Subscription { active: true },
        identity: 1,
    }
}
fn relay(window: Window) -> Window {
    window
}
fn frame(mut window: Window, command: u8) -> Window {
    let state = &mut window.model;
    if command == b'+' && state.count < 50 {
        state.count += 1;
        state.dirty = true;
    }
    if command == b'-' && state.count > -50 {
        state.count -= 1;
        state.dirty = true;
    }
    if command == b'r' {
        state.dirty = true;
    }
    let surface = &window.surface;
    let subscription = &window.subscription;
    let view = || state.count;
    if state.dirty && subscription.active && surface.width > 0 && surface.height > 0 {
        emit(format!("{}\n", view()).as_bytes());
        state.frames += 1;
        state.dirty = false;
    }
    window
}
fn finish(window: Window) -> i32 {
    emit(format!("#{}\n", window.model.frames).as_bytes());
    window.identity - 1
}
fn decode(code: u8) -> Result<u8, ()> {
    match code {
        b'+' | b'-' | b'r' | b'=' => Ok(code),
        _ => Err(()),
    }
}
fn application() -> Result<i32, ()> {
    let mut app = relay(create());
    app = frame(app, b'=');
    let mut input = io::stdin().lock();
    for _ in 0..128 {
        let mut code = [0];
        if input.read(&mut code).unwrap() == 0 || code[0] == b'q' {
            return Ok(finish(app));
        }
        if matches!(code[0], b'\n' | b'\r' | b' ') {
            continue;
        }
        app = frame(app, decode(code[0])?);
    }
    Err(())
}
fn main() {
    std::process::exit(application().unwrap_or(1));
}
