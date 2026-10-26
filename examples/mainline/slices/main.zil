import io
import sequence

// The same ordinary function also accepts a different backing-array length.
fn probe() -> bool {
    var short = [97u8, 98u8, 0u8]
    sequence.uppercase_ascii(&mut short[..])
    let tail = sequence.relay(&short[..], 1)
    return tail.len() == 2 && sequence.sum(tail) == 66
}

// Streaming byte processing with one bounded backing array. Unicode byte
// sequences and NUL are preserved; this is not a UTF-8 string abstraction.
fn main() -> i32 {
    if !probe() { return 3 }
    var buffer: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    var exhausted = false
    while !exhausted {
        var count: i64 = 0
        while count < 16 {
            let byte = io.read()
            if byte < 0 { exhausted = true; break }
            buffer[count] = u8(byte)
            count += 1
        }
        let window = &mut buffer[..count]
        sequence.uppercase_ascii(window)
        if io.emit(&*window) != 0 { return 2 }
    }
    return 0
}
