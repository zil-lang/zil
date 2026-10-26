import io
fn main() -> i32 {
    var buffer = io.empty()
    buffer[0] = 255u8
    if io.read_chunk(-1, &mut buffer) != -1 { return 1 }
    if buffer[0] != 255u8 { return 2 }
    if io.write_all(-1, &mut buffer, 1) != -1 { return 3 }
    if io.write_all(1, &mut buffer, 65) != -2 { return 4 }
    if io.write_all(1, &mut buffer, -1) != -2 { return 5 }
    if io.write_all(1, &mut buffer, 0) != 0 { return 6 }
    io.emit(69)
    return 0
}
