// Synchronous component update kernel; no platform window or GPU yet.
import io
import events
resource Model { count: i32; frames: i32; dirty: bool }
resource Surface { width: i32 }
resource Subscription { active: bool }
impl Drop for Model { fn drop(self: Self) { io.emit(77) } }
impl Drop for Surface { fn drop(self: Self) { io.emit(83) } }
impl Drop for Subscription { fn drop(self: Self) { io.emit(85) } }
package Window {
    manages model: Model
    manages surface: Surface
    manages subscription: Subscription
    serial: i32 = 0
}
fn create() -> Window {
    return Window {
        surface: Surface { width: 80 },
        model: Model { count: 0, frames: 0, dirty: true },
        subscription: Subscription { active: true }
    }
}
fn relay(window: Window) -> Window { return window }
fn refresh(give old: Surface) -> Surface contract { result(create) } {
    return Surface { width: old.width + 1 }
}
fn render(window: Window) -> Window {
    let state = window.model
    if state.dirty && window.surface.width > 0 && window.subscription.active {
        let view = () => state.count
        io.number(view())
        io.emit(10)
        state.frames += 1
        state.dirty = false
    }
    return window
}
fn finish(window: Window) -> i32 {
    io.emit(35)
    io.number(window.model.frames)
    io.emit(10)
    return 0
}
pub fn run() -> events.Fault!i32 {
    var app = relay(create())
    app = render(app)
    for i in 0..128 {
        let code = io.read()
        if code < 0 || code == 113 { return finish(app) }
        if code == 10 || code == 13 || code == 32 { continue }
        let command = events.decode(code)?
        let state = app.model
        if command == 43 && state.count < 50 { state.count += 1; state.dirty = true }
        if command == 45 && state.count > -50 { state.count -= 1; state.dirty = true }
        if command == 114 {
            app.surface = Surface { width: 80 }
            state.dirty = true
        }
        if command == 116 {
            app.surface = refresh(give app.surface)
            state.dirty = true
        }
        if command == 98 {
            if app.serial == 0 { let removed = give app.surface; app.serial = 1 }
            else { app.serial = 0 }
            // One path has an empty field; the other still owns the old one.
            app.surface = Surface { width: 80 }
            state.dirty = true
        }
        if command == 102 {
            // Preparation fails before old-surface cleanup.
            app.surface = Surface { width: 80 + events.prepare(command, 102)? }
        }
        // The sibling Model description stays valid across Surface changes.
        app = render(app)
    }
    return events.Fault.Limit
}
