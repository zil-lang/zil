//! Independent Rust behavior and cleanup oracle. No ZIL IR is consumed.
use std::io::{self, Read, Write};
fn emit(code: u8) {
    io::stdout().write_all(&[code]).unwrap();
}
fn number(value: i32) {
    io::stdout()
        .write_all(value.to_string().as_bytes())
        .unwrap();
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
fn token(code: u8, count: i32) -> Token {
    Token { code, count }
}
fn consume(item: Token) -> i32 {
    item.count
}
fn sample_twice<F: Fn() -> i32>(f: &F) -> i32 {
    let _ = f();
    f()
}
fn update_twice<F: FnMut() -> i32>(f: &mut F) -> i32 {
    let _ = f();
    f()
}
fn run_if<F: FnOnce() -> i32>(f: F, enabled: bool) -> i32 {
    if enabled { f() } else { 0 }
}
fn try_twice<F: FnMut() -> Result<i32, ()>>(f: &mut F) -> Result<i32, ()> {
    let first = f()?;
    Ok(first + f()?)
}
fn try_twice_shared<F: Fn() -> Result<i32, ()>>(f: &F) -> Result<i32, ()> {
    let first = f()?;
    Ok(first + f()?)
}
fn accept<F: FnOnce() -> i32>(f: F, item: Token, code: i32) -> i32 {
    emit(b'H');
    f() + item.count + code
}
fn check(stop: bool) -> Result<i32, ()> {
    if stop { Err(()) } else { Ok(0) }
}
fn shared() -> i32 {
    let a = token(b'A', 1);
    let f = move || {
        let held = &a;
        held.count
    };
    number(sample_twice(&f));
    number(f());
    0
}
fn exclusive() -> i32 {
    let mut a = token(b'A', 0);
    let mut f = move || {
        let held = &mut a;
        held.count += 1;
        held.count
    };
    number(update_twice(&mut f));
    number(f());
    0
}
fn conditional(enabled: bool) -> i32 {
    let a = token(b'A', 1);
    let b = token(b'B', 0);
    let f = move || {
        let result = consume(a);
        let held = &b;
        result + held.count
    };
    number(run_if(f, enabled));
    0
}
fn adaptation(exclusive: bool) -> i32 {
    let mut a = token(b'A', 3);
    if exclusive {
        let f = move || {
            let held = &mut a;
            held.count += 1;
            held.count
        };
        number(run_if(f, true));
        return 0;
    }
    let f = move || {
        let held = &a;
        held.count
    };
    number(run_if(f, true));
    0
}
fn shared_as_mut() -> i32 {
    let a = token(b'A', 3);
    let mut f = move || {
        let held = &a;
        held.count
    };
    number(update_twice(&mut f));
    number(f());
    0
}
fn copy_as_once() -> i32 {
    let n = 7;
    let f = || n;
    number(run_if(f, true));
    0
}
fn prepared(stop: bool) -> Result<i32, ()> {
    let _outer = token(b'Z', 0);
    let a = token(b'A', 1);
    let f = move || consume(a);
    let b = token(b'B', 2);
    let result = accept(f, b, check(stop)?);
    number(result);
    Ok(0)
}
fn errors(stop: i32) -> Result<i32, ()> {
    let mut a = token(b'A', 0);
    let mut f = move || {
        let held = &mut a;
        held.count += 1;
        number(held.count);
        if held.count == stop {
            Err(())
        } else {
            Ok(held.count)
        }
    };
    let result = try_twice(&mut f)?;
    emit(b':');
    number(result);
    emit(b':');
    number(f()?);
    Ok(0)
}
fn shared_errors(stop: i32) -> Result<i32, ()> {
    // RefCell is only this Rust oracle's explicit way to represent a shared
    // closure writing an external Desc target; it is not emitted by ZIL.
    let a = std::cell::RefCell::new(token(b'A', 0));
    let f = || {
        let mut held = a.borrow_mut();
        held.count += 1;
        number(held.count);
        if held.count == stop {
            Err(())
        } else {
            Ok(held.count)
        }
    };
    let result = try_twice_shared(&f)?;
    emit(b':');
    number(result);
    Ok(0)
}
fn main() {
    let mut code = [0];
    let _ = io::stdin().read(&mut code).unwrap();
    let result = match code[0] {
        b's' => Ok(shared()),
        b'm' => Ok(exclusive()),
        b'0' => Ok(conditional(false)),
        b'1' => Ok(conditional(true)),
        b'a' => Ok(adaptation(false)),
        b'b' => Ok(adaptation(true)),
        b'o' => Ok(shared_as_mut()),
        b'c' => Ok(copy_as_once()),
        b'p' => prepared(true),
        b'P' => prepared(false),
        b'e' => errors(1),
        b'f' => errors(2),
        b'E' => errors(0),
        b'd' => shared_errors(1),
        b'D' => shared_errors(0),
        _ => Err(()),
    };
    std::process::exit(result.unwrap_or(1));
}
