//! Independent native behavior oracle; it is not a ZIL execution backend.
use std::process::ExitCode;

unsafe extern "C" {
    fn getchar() -> i32;
    fn putchar(value: i32) -> i32;
}

struct Token {
    code: i32,
}
impl Drop for Token {
    fn drop(&mut self) {
        // The example observes successful byte IO, under the same C ABI as ZIL.
        unsafe {
            putchar(self.code);
        }
    }
}
fn make(code: i32) -> Token {
    Token { code }
}
fn relay(token: Token) -> Token {
    token
}
fn bump(token: &mut Token, amount: i32) {
    token.code += amount;
}
fn read(token: &Token) -> i32 {
    token.code
}
fn consume(mut token: Token, amount: i32) -> i32 {
    bump(&mut token, amount);
    token.code
}
fn run() -> u8 {
    let command = unsafe { getchar() };
    let _outer = make(65);
    {
        let mut x = make(66);
        let alias = &mut x;
        bump(alias, 1);
        let y = relay(x);
        let result = consume(make(68), 1);
        if result != 69 {
            return 3;
        }
        read(&make(73));
        if command == 33 {
            return 2;
        }
        if read(&y) != 67 {
            return 4;
        }
    }
    for i in 0..4 {
        let _item = make(70 + i);
        if i == 0 {
            continue;
        }
        if i == 1 {
            break;
        }
    }
    let _tail = make(72);
    unsafe {
        putchar(124);
    }
    0
}
fn main() -> ExitCode {
    ExitCode::from(run())
}
