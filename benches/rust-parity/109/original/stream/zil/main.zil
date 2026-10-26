import io
import transform
fn run() -> i32 {
    var input = io.read_byte()
    while input >= 0 {
        io.write_byte(transform.upper(u8(input)))
        input = io.read_byte()
    }
    return 0
}
fn main() -> i32 { return run() }
