import io
import lifetime
pub enum Fault { Stop(i32) }
fn step(code: i32) -> Fault!i32 {
    let second = lifetime.token(66)
    io.emit(50)
    if code == 101 { return Fault.Stop(code) }
    return 7
}
pub fn work(code: i32) -> Fault!i32 {
    let first = lifetime.token(65)
    io.emit(49)
    let result = step(code)?
    io.emit(51)
    return result
}
// Importing this module must not execute its own main or any unused function.
pub fn main() -> i32 { io.emit(88); return 99 }
pub fn unused() { io.emit(89) }
