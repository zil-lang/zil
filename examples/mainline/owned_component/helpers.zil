import events
pub fn apply<F: FnMut(i32) -> events.Fault!i32>(f: &mut F, command: i32) -> events.Fault!i32 {
    return f(command)
}
