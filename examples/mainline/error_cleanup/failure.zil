import io

pub enum Fault { Stop(i32) }
pub fn check(stage: i32, stop: i32) -> Fault!i32 {
    if stage == stop { return Fault.Stop(stop) }
    return 0
}
pub fn value(fail: bool) -> Fault!i32 {
    io.emit(118)
    if fail { return Fault.Stop(4) }
    return 0
}
