pub enum Fault { Invalid(i32), Limit, Stop(i32) }
pub fn check(stop: bool) -> Fault!i32 {
    if stop { return Fault.Stop(0) }
    return 0
}
pub fn delta(code: i32) -> Fault!i32 {
    if code == 43 { return 1 }
    if code == 45 { return -1 }
    if code == 61 { return 0 }
    return Fault.Invalid(code)
}
