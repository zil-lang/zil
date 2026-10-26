import events
pub fn sample_twice<F: Fn() -> i32>(f: &F) -> i32 { f(); return f() }
pub fn update_twice<F: FnMut(i32) -> i32>(f: &mut F) -> i32 { f(1); return f(1) }
pub fn run_if<F: FnOnce() -> i32>(f: F, yes: bool) -> i32 {
    if yes { return f() }
    return 0
}
pub fn try_twice<F: FnMut() -> events.Fault!i32>(f: &mut F) -> events.Fault!i32 {
    let first = f()?
    return first + f()?
}
