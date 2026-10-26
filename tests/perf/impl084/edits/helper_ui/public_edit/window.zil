// A bounded synchronous resource kernel, without a platform window or GPU.
import io
import helpers

resource Model { count: i32; frames: i32; dirty: bool }
resource Surface { width: i32; height: i32 }
resource Subscription { active: bool }
impl Drop for Model { fn drop(self: Self) { io.emit(77) } }
impl Drop for Surface { fn drop(self: Self) { io.emit(83) } }
impl Drop for Subscription { fn drop(self: Self) { io.emit(85) } }

pub package Window {
    manages model: Model
    manages surface: Surface
    manages subscription: Subscription
    identity: i32 = 1
}

pub fn create() -> Window {
    // Written order differs from declaration/drop order.
    return Window {
        surface: Surface { width: 80, height: 24 },
        model: Model { count: 0, frames: 0, dirty: true },
        subscription: Subscription { active: true }
    }
}

pub fn relay(window: Window) -> Window { return window }

pub fn frame(window: Window, command: i32) -> Window {
    let state = window.model
    if command == 43 && state.count < 50 {
        state.count = state.count + 1
        state.dirty = true
    }
    if command == 45 && state.count > -50 {
        state.count = state.count - 1
        state.dirty = true
    }
    if command == 114 { state.dirty = true }
    let surface = window.surface
    let subscription = window.subscription
    // Capture an actual field description. No package enters the environment.
    let view = () => state.count
    if state.dirty && subscription.active && surface.width > 0 && surface.height > 0 {
        io.number(helpers.sample_twice(&view), 0)
        io.emit(10)
        state.frames = state.frames + 1
        state.dirty = false
    }
    return window
}

pub fn finish(window: Window) -> i32 {
    let state = window.model
    io.emit(35)
    io.number(state.frames, 0)
    io.emit(10)
    return window.identity - 1
}
