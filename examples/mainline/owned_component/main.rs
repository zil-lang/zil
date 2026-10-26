// Independent behavior model; no ZIL parser, IR or proof routine is used.
use std::io::{self, Read};
struct Model {
    count: i32,
    frames: i32,
    dirty: bool,
}
struct Surface {
    width: i32,
}
struct Subscription {
    active: bool,
}
impl Drop for Model {
    fn drop(&mut self) {
        print!("M");
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        print!("S");
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        print!("U");
    }
}
struct Window {
    model: Model,
    surface: Option<Surface>,
    subscription: Subscription,
    serial: i32,
}
fn refresh(old: Surface) -> Surface {
    Surface {
        width: old.width + 1,
    }
}
fn handler() -> impl FnMut(i32) -> Result<i32, ()> {
    let mut app = Window {
        model: Model {
            count: 0,
            frames: 0,
            dirty: true,
        },
        surface: Some(Surface { width: 80 }),
        subscription: Subscription { active: true },
        serial: 0,
    };
    move |code| {
        // The whole borrow keeps capture/drop order aligned with a single component.
        let app = &mut app;
        if code == 35 {
            return Ok(app.model.frames);
        }
        if !matches!(code, 43 | 45 | 114 | 116 | 98 | 102 | 61) {
            return Err(());
        }
        let state = &mut app.model;
        if code == 43 && state.count < 50 {
            state.count += 1;
            state.dirty = true;
        }
        if code == 45 && state.count > -50 {
            state.count -= 1;
            state.dirty = true;
        }
        if code == 114 {
            app.surface = Some(Surface { width: 80 });
            state.dirty = true;
        }
        if code == 116 {
            app.surface = Some(refresh(app.surface.take().unwrap()));
            state.dirty = true;
        }
        if code == 98 {
            if app.serial == 0 {
                let _removed = app.surface.take().unwrap();
                app.serial = 1;
            } else {
                app.serial = 0;
            }
            app.surface = Some(Surface { width: 80 });
            state.dirty = true;
        }
        if code == 102 {
            return Err(());
        }
        if state.dirty && app.surface.as_ref().unwrap().width > 0 && app.subscription.active {
            state.frames += 1;
            state.dirty = false;
        }
        Ok(state.count)
    }
}
fn apply<F: FnMut(i32) -> Result<i32, ()>>(f: &mut F, code: i32) -> Result<i32, ()> {
    f(code)
}
fn read() -> i32 {
    let mut b = [0];
    if io::stdin().read(&mut b).unwrap() == 0 {
        -1
    } else {
        i32::from(b[0])
    }
}
fn run() -> Result<(), ()> {
    let mut update = handler();
    println!("{}", apply(&mut update, 61)?);
    for _ in 0..128 {
        let code = read();
        if code < 0 || code == 113 {
            println!("#{}", apply(&mut update, 35)?);
            return Ok(());
        }
        if matches!(code, 10 | 13 | 32) {
            continue;
        }
        println!("{}", apply(&mut update, code)?);
    }
    Err(())
}
fn main() -> std::process::ExitCode {
    if run().is_ok() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
