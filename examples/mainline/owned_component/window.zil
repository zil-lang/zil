// Entire component stays inside one returned, inline owning environment.
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
fn refresh(give old: Surface) -> Surface contract { result(create) } {
    return Surface { width: old.width + 1 }
}
pub fn handler() -> impl FnMut(i32) -> events.Fault!i32 {
    var app = create()
    return move (code) => {
        if code == 35 { return app.model.frames }
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
            app.surface = Surface { width: 80 }
            state.dirty = true
        }
        if command == 102 {
            // Error exits while the original field is still complete.
            app.surface = Surface { width: 80 + events.prepare(command, 102)? }
        }
        if state.dirty && app.surface.width > 0 && app.subscription.active {
            state.frames += 1
            state.dirty = false
        }
        return state.count
    }
}
