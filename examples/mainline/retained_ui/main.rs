//! Independent native oracle, written against behavior rather than ZIL IR.
use std::io::{self, Read, Write};
fn emit(value: u8) {
    io::stdout().write_all(&[value]).unwrap();
}
fn number(value: i32) {
    io::stdout()
        .write_all(value.to_string().as_bytes())
        .unwrap();
}
fn read() -> Option<u8> {
    let mut byte = [0];
    (io::stdin().read(&mut byte).unwrap() != 0).then_some(byte[0])
}
struct Token {
    code: u8,
    count: i32,
}
impl Drop for Token {
    fn drop(&mut self) {
        emit(self.code);
    }
}
fn consume(value: Token) -> i32 {
    value.count
}
fn reader(seed: i32) -> impl Fn() -> i32 {
    let token = Token {
        code: b'B',
        count: seed,
    };
    move || {
        let held = &token;
        held.count
    }
}
fn counter(seed: i32) -> impl FnMut(i32) -> i32 {
    let _temporary = Token {
        code: b'F',
        count: 0,
    };
    let mut token = Token {
        code: b'A',
        count: seed,
    };
    move |delta| {
        let held = &mut token;
        held.count += delta;
        held.count
    }
}
fn once(seed: i32) -> impl FnOnce() -> i32 {
    let first = Token {
        code: b'C',
        count: seed,
    };
    let second = Token {
        code: b'D',
        count: 0,
    };
    move || {
        let result = consume(first);
        let held = &second;
        result + held.count
    }
}
fn fallible(stop: i32) -> impl FnMut() -> Result<i32, ()> {
    let mut token = Token {
        code: b'E',
        count: 0,
    };
    move || {
        let held = &mut token;
        held.count += 1;
        number(held.count);
        if held.count == stop {
            Err(())
        } else {
            Ok(held.count)
        }
    }
}
fn snapshot(seed: i32) -> impl Fn(i32) -> i32 {
    move |value| seed + value
}
fn sample_twice<F: Fn() -> i32>(f: &F) -> i32 {
    let _ = f();
    f()
}
fn update_twice<F: FnMut(i32) -> i32>(f: &mut F) -> i32 {
    let _ = f(1);
    f(1)
}
fn run_if<F: FnOnce() -> i32>(f: F, yes: bool) -> i32 {
    if yes { f() } else { 0 }
}
fn try_twice<F: FnMut() -> Result<i32, ()>>(f: &mut F) -> Result<i32, ()> {
    let first = f()?;
    Ok(first + f()?)
}
struct Guard {
    code: u8,
    value: i32,
}
impl Drop for Guard {
    fn drop(&mut self) {
        emit(self.code);
    }
}
fn retain(guard: Guard) -> impl Fn() -> i32 {
    move || {
        let held = &guard;
        held.value
    }
}
fn accept<F: FnOnce() -> i32>(f: F, guard: Guard, code: i32) -> i32 {
    emit(b'H');
    f() + guard.value + code
}
fn check(stop: bool) -> Result<i32, ()> {
    if stop { Err(()) } else { Ok(0) }
}
fn prepared(stop: bool) -> Result<i32, ()> {
    let _outer = Guard {
        code: b'Z',
        value: 0,
    };
    let f = once(1);
    let item = Guard {
        code: b'B',
        value: 2,
    };
    let value = accept(f, item, check(stop)?);
    number(value);
    Ok(0)
}
fn errors(stop: i32) -> Result<i32, ()> {
    let mut f = fallible(stop);
    let value = try_twice(&mut f)?;
    emit(b':');
    number(value);
    emit(b':');
    number(f()?);
    Ok(0)
}
fn delta(code: u8) -> Result<i32, ()> {
    match code {
        b'+' => Ok(1),
        b'-' => Ok(-1),
        b'=' => Ok(0),
        _ => Err(()),
    }
}
fn event_loop() -> Result<i32, ()> {
    let mut update = counter(0);
    number(update(0));
    emit(b'\n');
    for _ in 0..32 {
        let Some(code) = read() else { return Ok(0) };
        if code == b'q' {
            return Ok(0);
        }
        if matches!(code, b'\n' | b' ') {
            continue;
        }
        number(update(delta(code)?));
        emit(b'\n');
    }
    Err(())
}
fn application() -> Result<i32, ()> {
    match read() {
        Some(b'l') => event_loop(),
        Some(b's') => {
            let f = reader(21);
            number(sample_twice(&f));
            let moved = f;
            number(moved());
            Ok(0)
        }
        Some(b'z') => {
            let _unused = reader(1);
            emit(b'Z');
            Ok(0)
        }
        Some(b'm') => {
            let mut f = counter(0);
            number(update_twice(&mut f));
            number(f(1));
            Ok(0)
        }
        Some(mode @ (b'0' | b'1')) => {
            let f = once(1);
            number(run_if(f, mode == b'1'));
            Ok(0)
        }
        Some(b'p') => prepared(true),
        Some(b'P') => prepared(false),
        Some(b'e') => errors(1),
        Some(b'f') => errors(2),
        Some(b'E') => errors(0),
        Some(b'c') => {
            let f = snapshot(20);
            number(f(1) + f(1));
            Ok(0)
        }
        Some(b'g') => {
            let guard = Guard {
                code: b'G',
                value: 7,
            };
            let f = retain(guard);
            number(f());
            number(f());
            Ok(0)
        }
        _ => Err(()),
    }
}
fn main() {
    std::process::exit(application().unwrap_or(1));
}
