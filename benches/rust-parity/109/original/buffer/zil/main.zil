import buffer
import sequence
import io
fn run() -> i32 {
    let input = buffer.create()
    var code = io.read_byte()
    while code >= 0 {
        if !buffer.append(input, u8(code)) { return 2 }
        code = io.read_byte()
    }
    let ready = buffer.relay(give input)
    sequence.uppercase(buffer.view_mut(ready))
    let output = buffer.view(ready)
    var index: i64 = 0
    while index < output.len() {
        io.write_byte(output[index])
        index += 1
    }
    return 0
}
fn main() -> i32 { return run() }
