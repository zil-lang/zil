import io
// Streaming ASCII normalization preserves every other byte, including NUL and
// UTF-8 sequences. The input length is unbounded; storage is one fixed buffer.
fn main() -> i32 {
    var buffer = io.empty()
    loop {
        let count = io.read_chunk(0, &mut buffer)
        if count < 0 { return 1 }
        if count == 0 { break }
        var index = 0
        while index < count {
            let byte = buffer[index]
            if byte >= 97u8 && byte <= 122u8 { buffer[index] = byte - 32u8 }
            index = index + 1
        }
        if io.write_all(1, &mut buffer, count) != 0 { return 2 }
    }
    return 0
}
