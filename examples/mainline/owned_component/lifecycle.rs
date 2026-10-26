// Independent lifecycle model; fixed observations are also pinned by the harness.
use std::io::{self, Read};
struct Token {
    code: i32,
}
impl Drop for Token {
    fn drop(&mut self) {
        print!("{}", char::from(self.code as u8));
    }
}
struct Group {
    first: Option<Token>,
    second: Option<Token>,
    third: Option<Token>,
    count: i32,
}
impl Group {
    fn code(&self) -> i32 {
        assert!(self.third.is_some());
        self.first.as_ref().unwrap().code + self.count
    }
}
fn make() -> Group {
    Group {
        first: Some(Token { code: 65 }),
        second: Some(Token { code: 66 }),
        third: Some(Token { code: 67 }),
        count: 0,
    }
}
fn reader(group: Group) -> impl Fn() -> i32 {
    move || group.code()
}
fn updater() -> impl FnMut(i32) -> Result<i32, ()> {
    let mut group = Some(make());
    move |stage| {
        if stage == 0 {
            return Err(());
        }
        if stage == 4 {
            let whole = group.take().unwrap();
            group = Some(whole);
        }
        let group = group.as_mut().unwrap();
        if stage == 3 {
            group.first = Some(Token { code: 68 });
        }
        if stage == 7 {
            for _ in 0..3 {
                let old = group.first.take().unwrap();
                group.first = Some(Token { code: old.code + 1 });
            }
        }
        Ok(group.code())
    }
}
fn partial() -> impl FnOnce() -> i32 {
    let before = Token { code: 88 };
    let mut group = make();
    let after = Token { code: 89 };
    move || {
        let n = {
            let held = &before;
            held.code - 88
        };
        let group = &mut group;
        let removed = group.second.take().unwrap();
        n + removed.code - 66 + {
            let held = &after;
            held.code - 89
        }
    }
}
fn whole() -> impl FnOnce() -> i32 {
    let before = Token { code: 88 };
    let group = make();
    let after = Token { code: 89 };
    move || {
        let n = {
            let held = &before;
            held.code - 88
        };
        let moved = group;
        n + moved.count + {
            let held = &after;
            held.code - 89
        }
    }
}
fn fallible() -> impl FnOnce() -> Result<i32, ()> {
    let mut group = make();
    move || {
        let group = &mut group;
        let _removed = group.second.take().unwrap();
        Err(())
    }
}
fn run() -> Result<i32, ()> {
    let mut input = [0];
    let size = io::stdin().read(&mut input).unwrap();
    let code = if size == 0 { 0 } else { input[0] };
    let _outer = Token { code: 90 };
    match code {
        b'r' => {
            let f = reader(make());
            print!("{}{}", f(), f());
            let moved = f;
            return Ok(moved() - 65);
        }
        b'u' => {
            let _unused = reader(make());
            return Ok(0);
        }
        b'0' => {
            let _f = partial();
            return Ok(0);
        }
        b'1' => {
            let f = partial();
            print!("{}", f());
            return Ok(0);
        }
        b'2' => {
            let f = whole();
            print!("{}", f());
            return Ok(0);
        }
        b'e' => {
            let f = fallible();
            return f();
        }
        _ => {}
    }
    let mut update = updater();
    match code {
        b'f' => {
            let _failed = update(0);
            print!("X");
            print!("{}", update(3)?);
            Ok(0)
        }
        b'4' => {
            print!("{}", update(4)?);
            print!("{}", update(4)?);
            Ok(0)
        }
        b'7' => {
            print!("{}", update(7)?);
            print!("{}", update(7)?);
            Ok(0)
        }
        _ => update(0),
    }
}
fn main() -> std::process::ExitCode {
    if run().is_ok() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
