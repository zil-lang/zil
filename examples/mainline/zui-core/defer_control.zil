// Two-module baseline observation only; no new Product shape.
import portable
fn main() -> i32 {
    defer { portable.emit(81) }
    return 0
}
