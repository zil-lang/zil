//! Independent preparation-versus-complete-drop oracle.
use std::io::{self, Read, Write};
fn emit(code: u8) {
    io::stdout().write_all(&[code]).unwrap();
}
struct Token(u8);
impl Drop for Token {
    fn drop(&mut self) {
        emit(self.0);
    }
}
#[allow(dead_code)]
struct Group {
    first: Token,
    second: Token,
    third: Token,
    count: i32,
}
fn check(stage: i32, stop: i32) -> Result<i32, ()> {
    if stage == stop { Err(()) } else { Ok(0) }
}
fn relay(group: Group) -> Group {
    group
}
fn finish(group: Group, extra: i32) -> i32 {
    emit(b'X');
    group.count + extra - 3
}
fn prepare(stage: i32) -> Result<i32, ()> {
    let _outer = Token(b'Z');
    let group = Group {
        third: Token(b'C' + check(stage, 0)? as u8),
        first: Token(b'A' + check(stage, 1)? as u8),
        second: Token(b'B' + check(stage, 2)? as u8),
        count: 3 + check(stage, 3)?,
    };
    let moved = relay(group);
    Ok(finish(moved, check(stage, 4)?))
}
fn loops() -> i32 {
    for i in 0..3 {
        let _group = Group {
            third: Token(b'C'),
            first: Token(b'A'),
            second: Token(b'B'),
            count: 3,
        };
        if i == 0 {
            continue;
        }
        if i == 1 {
            break;
        }
    }
    0
}
fn main() {
    let mut input = [0];
    let _ = io::stdin().read(&mut input).unwrap();
    let result = if input[0] == b'l' {
        Ok(loops())
    } else {
        prepare(input[0] as i32 - 48)
    };
    std::process::exit(result.unwrap_or(1));
}
