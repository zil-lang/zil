import state
import io
fn run() -> i32 {
    var update = state.make()
    var input = io.read_byte()
    while input >= 0 {
        let result = update(u8(input))
        if result < 0 { return 2 }
        io.write_byte(u8(result))
        input = io.read_byte()
    }
    return 0
}
fn main() -> i32 { return run() }
