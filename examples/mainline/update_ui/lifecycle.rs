// No ZIL IR or solver is reused; fixed expectations are checked by the harness.
use std::io::{self, Read};
struct Token {
    code: u8,
}
impl Drop for Token {
    fn drop(&mut self) {
        print!("{}", char::from(self.code));
    }
}
struct Group {
    first: Option<Token>,
    second: Option<Token>,
    third: Option<Token>,
    count: i32,
}
fn make() -> Group {
    Group {
        first: Some(Token { code: b'A' }),
        second: Some(Token { code: b'B' }),
        third: Some(Token { code: b'C' }),
        count: 0,
    }
}
fn relay(group: Group) -> Group {
    group
}
fn keep(token: Token, _n: i32) -> Token {
    token
}
fn combine(token: Token, _old: Token, _n: i32) -> Token {
    token
}
fn prepare(stage: i32, stop: i32) -> Result<i32, ()> {
    if stage == stop { Err(()) } else { Ok(0) }
}
fn conditional(yes: bool) {
    let mut group = make();
    if yes {
        let _old = group.first.take().unwrap();
    }
    group.first = Some(Token { code: b'D' });
}
fn exercise(stage: i32) -> Result<i32, ()> {
    let _outer = Token { code: b'Z' };
    let mut group = make();
    match stage {
        0 => {
            group.first = Some(Token {
                code: b'D' + prepare(stage, 0)? as u8,
            })
        }
        1 => group.first = Some(keep(Token { code: b'D' }, prepare(stage, 1)?)),
        2 => group.first = Some(keep(group.first.take().unwrap(), prepare(stage, 2)?)),
        3 => group.first = Some(Token { code: b'D' }),
        4 => {
            let a = group.first.take().unwrap();
            let b = group.second.take().unwrap();
            group.first = Some(b);
            group.second = Some(a);
            let moved = relay(group);
            return Ok(moved.count);
        }
        5 => {
            let _a = group.first.take().unwrap();
            let _b = group.second.take().unwrap();
            let _c = group.third.take().unwrap();
            group.count = 1;
            return Ok(group.count - 1);
        }
        7 => {
            for _ in 0..3 {
                let old = group.first.take().unwrap();
                group.first = Some(Token { code: old.code + 1 });
            }
        }
        8 => group.first = Some(keep(group.first.take().unwrap(), 0)),
        9 => {
            group.first = Some(combine(
                Token { code: b'D' },
                group.first.take().unwrap(),
                prepare(stage, 9)?,
            ))
        }
        _ => {}
    }
    Ok(0)
}
fn main() -> std::process::ExitCode {
    let mut input = [0u8];
    let n = io::stdin().read(&mut input).unwrap();
    if n == 1 && input[0] == b'6' {
        conditional(true);
        conditional(false);
        return std::process::ExitCode::SUCCESS;
    }
    let stage = if n == 1 {
        i32::from(input[0]) - 48
    } else {
        -49
    };
    if exercise(stage).is_ok() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
