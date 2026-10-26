//! Independent Rust lifecycle oracle; no code is generated from the ZIL IR.
use std::io::{self, Read, Write};
struct Token(i32);
impl Drop for Token {
    fn drop(&mut self) { io::stdout().write_all(&[self.0 as u8]).unwrap(); }
}
#[derive(Debug)]
struct Fault;
fn check(stage: i32, stop: i32) -> Result<i32, Fault> {
    if stage == stop { Err(Fault) } else { Ok(0) }
}
fn value(fail: bool) -> Result<i32, Fault> {
    io::stdout().write_all(b"v").unwrap();
    if fail { Err(Fault) } else { Ok(0) }
}
fn read(token: &Token) -> i32 { token.0 }
fn accept(token: Token, amount: i32) -> i32 {
    io::stdout().write_all(b"X").unwrap();
    token.0 + amount
}
fn transaction(stage: i32) -> Result<i32, Fault> {
    let _outer = Token(65);
    check(stage, 0)?;
    let item = Token(66);
    let callback = move || read(&item);
    check(stage, 1)?;
    let _tail = Token(67);
    check(stage, 2)?;
    if callback() != 66 { return Err(Fault); }
    Ok(7)
}
fn pending_argument(fail: bool) -> Result<i32, Fault> {
    let _outer = Token(65);
    let argument = Token(66);
    let answer = accept(argument, value(fail)?);
    Ok(answer - 66)
}
fn pending_callee(fail: bool) -> Result<i32, Fault> {
    let _outer = Token(65);
    let item = Token(66);
    let once = move |amount| accept(item, amount);
    let answer = once(value(fail)?);
    Ok(answer - 66)
}
fn partial_constructor(fail: bool) -> Result<i32, Fault> {
    let _outer = Token(65);
    let item = Token(66 + value(fail)?);
    Ok(read(&item) - 66)
}
fn closure_error(fail: bool) -> Result<i32, Fault> {
    let _outer = Token(65);
    let item = Token(66);
    let once = move || {
        if fail { check(0, 0)?; }
        accept(item, 0);
        Ok(0)
    };
    once()
}
fn loop_error(stage: i32) -> Result<i32, Fault> {
    let _outer = Token(65);
    for i in 0..3 {
        let item = Token(66 + i);
        let callback = move || read(&item);
        if i == 0 { continue; }
        check(i, stage)?;
        if callback() == 68 { break; }
    }
    Ok(0)
}
fn main() {
    let mut input = [0];
    let _ = io::stdin().read(&mut input).unwrap();
    let result = match input[0] {
        b'0'..=b'3' => transaction((input[0] - b'0') as i32),
        b'A' => pending_argument(true), b'a' => pending_argument(false),
        b'F' => pending_callee(true), b'f' => pending_callee(false),
        b'P' => partial_constructor(true), b'p' => partial_constructor(false),
        b'C' => closure_error(true), b'c' => closure_error(false),
        b'L' => loop_error(1), b'l' => loop_error(9), _ => Err(Fault),
    };
    std::process::exit(result.unwrap_or(1));
}
