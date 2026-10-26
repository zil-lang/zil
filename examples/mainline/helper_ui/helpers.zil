import events

pub fn sample_twice<F: Fn() -> i32>(f: &F) -> i32 {
    f()
    return f()
}
pub fn update_twice<F: FnMut() -> i32>(f: &mut F) -> i32 {
    f()
    return f()
}
pub fn run_if<F: FnOnce() -> i32>(f: F, enabled: bool) -> i32 {
    if enabled { return f() }
    return 0
}
pub fn try_twice<F: Fn() -> events.Fault!i32>(f: &F) -> events.Fault!i32 {
    let first = f()?
    return first + f()?
}
pub fn try_twice_mut<F: FnMut() -> events.Fault!i32>(f: &mut F) -> events.Fault!i32 {
    let first = f()?
    return first + f()?
}
