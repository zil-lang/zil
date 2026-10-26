//! Independent lifecycle oracle. Cell models ZIL's sequential resource Desc
//! access without granting Rust aliased &mut or adding a heap allocation.
use std::{cell::Cell, io::{self, Read, Write}};
struct Token { code: Cell<i32> }
impl Drop for Token {
    fn drop(&mut self) { io::stdout().write_all(&[self.code.get() as u8]).unwrap(); }
}
fn make(code: i32) -> Token { Token { code: Cell::new(code) } }
fn read(token: &Token) -> i32 { token.code.get() }
fn bump(token: &Token, amount: i32) { token.code.set(read(token) + amount); }
fn consume(token: Token, amount: i32) -> i32 { bump(&token, amount); read(&token) }
fn shared<F: Fn() -> i32>(f: &F) -> i32 { f() }
fn mutable<F: FnMut() -> i32>(f: &mut F) -> i32 { f() }
fn once<F: FnOnce() -> i32>(f: F) -> i32 { f() }
fn described() -> i32 {
    let r = make(65);
    let f = || read(&r);
    if shared(&f) != 65 { return 1; }
    bump(&r, 1);
    if shared(&f) != 66 { return 2; }
    let write = || bump(&r, 1);
    write();
    if shared(&f) != 67 { return 3; }
    0
}
fn owned_shared() -> i32 {
    let r = make(65);
    let f = move || read(&r);
    if shared(&f) != 65 { return 1; }
    if shared(&f) != 65 { return 2; }
    0
}
fn uncalled() -> i32 {
    let r = make(65);
    let s = make(66);
    let _unused = move || { read(&r); read(&s) };
    0
}
fn consumed() -> i32 {
    let r = make(65);
    let s = make(66);
    let f = move || { read(&s); consume(r, 2); 9 };
    if once(f) != 9 { return 1; }
    0
}
fn owned_mutable() -> i32 {
    let r = make(65);
    let mut f = move || { bump(&r, 1); read(&r) };
    if mutable(&mut f) != 66 { return 1; }
    if mutable(&mut f) != 67 { return 2; }
    0
}
fn early() -> i32 {
    let r = make(65);
    let _unused = move || read(&r);
    7
}
fn transferred() -> i32 {
    let r = make(65);
    let first = move || read(&r);
    let second = first;
    if shared(&second) != 65 { return 1; }
    0
}
fn loop_exit() -> i32 {
    for i in 0..3 {
        let r = make(65 + i);
        let f = move || read(&r);
        if i == 0 { continue; }
        if shared(&f) != 66 { return 1; }
        break;
    }
    0
}
fn transaction(stage: i32) -> Result<i32, i32> {
    let _outer = make(65);
    if stage == 0 { return Err(1); }
    let item = make(66);
    let callback = move || read(&item);
    if stage == 1 { return Err(2); }
    let _tail = make(67);
    if stage == 2 { return Err(3); }
    if shared(&callback) != 66 { return Err(4); }
    Ok(0)
}
fn main() {
    let mut input = [0];
    let _ = io::stdin().read(&mut input).unwrap();
    let result = match input[0] {
        b'0'..=b'3' => transaction((input[0] - b'0') as i32).unwrap_or_else(|error| error),
        b'D' => described(), b'S' => owned_shared(), b'U' => uncalled(),
        b'O' => consumed(), b'M' => owned_mutable(), b'E' => early(),
        b'T' => transferred(), b'L' => loop_exit(), _ => 8,
    };
    std::process::exit(result);
}
