pub enum Fault { Invalid(i32), Limit, Prepare(i32) }
pub fn decode(code: i32) -> Fault!i32 {
    if code == 43 || code == 45 || code == 114 || code == 116 || code == 98 || code == 102 || code == 61 { return code }
    return Fault.Invalid(code)
}
pub fn prepare(stage: i32, stop: i32) -> Fault!i32 {
    if stage == stop { return Fault.Prepare(stop) }
    return 0
}
