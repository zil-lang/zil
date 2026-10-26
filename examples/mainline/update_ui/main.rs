// Independent behavioral oracle: ordinary Rust values and explicit Option moves.
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
fn create() -> Window {
    Window {
        model: Model {
            count: 0,
            frames: 0,
            dirty: true,
        },
        surface: Some(Surface { width: 80 }),
        subscription: Subscription { active: true },
        serial: 0,
    }
}
fn relay(window: Window) -> Window {
    window
}
fn refresh(old: Surface) -> Surface {
    Surface {
        width: old.width + 1,
    }
}
fn render(mut window: Window) -> Window {
    if window.model.dirty
        && window.surface.as_ref().unwrap().width > 0
        && window.subscription.active
    {
        let view = || window.model.count;
        println!("{}", view());
        window.model.frames += 1;
        window.model.dirty = false;
    }
    window
}
fn finish(window: Window) {
    println!("#{}", window.model.frames);
}
fn prepare(stage: u8, stop: u8) -> Result<i32, ()> {
    if stage == stop { Err(()) } else { Ok(0) }
}
fn run(input: &[u8]) -> Result<(), ()> {
    let mut app = render(relay(create()));
    for i in 0..128 {
        let Some(&command) = input.get(i) else {
            finish(app);
            return Ok(());
        };
        if command == b'q' {
            finish(app);
            return Ok(());
        }
        if matches!(command, b'\n' | b'\r' | b' ') {
            continue;
        }
        if !matches!(command, b'+' | b'-' | b'r' | b't' | b'b' | b'f' | b'=') {
            return Err(());
        }
        let state = &mut app.model;
        match command {
            b'+' if state.count < 50 => {
                state.count += 1;
                state.dirty = true;
            }
            b'-' if state.count > -50 => {
                state.count -= 1;
                state.dirty = true;
            }
            b'r' => {
                app.surface = Some(Surface { width: 80 });
                state.dirty = true;
            }
            b't' => {
                app.surface = Some(refresh(app.surface.take().unwrap()));
                state.dirty = true;
            }
            b'b' => {
                if app.serial == 0 {
                    let _removed = app.surface.take().unwrap();
                    app.serial = 1;
                } else {
                    app.serial = 0;
                }
                app.surface = Some(Surface { width: 80 });
                state.dirty = true;
            }
            b'f' => {
                app.surface = Some(Surface {
                    width: 80 + prepare(command, b'f')?,
                });
            }
            _ => {}
        }
        app = render(app);
    }
    Err(())
}
fn main() -> std::process::ExitCode {
    let mut input = Vec::new();
    io::stdin().read_to_end(&mut input).unwrap();
    if run(&input).is_ok() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
