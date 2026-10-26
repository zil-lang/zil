import io
// Executed with the explicit C short-write fixture linked by the test runner.
extern "C" fn getchar() -> i32;
fn main() -> i32 {
    var mode = 0
    unsafe { mode = getchar() }
    var buffer = io.empty()
    buffer[0] = 0u8; buffer[1] = 127u8; buffer[2] = 128u8; buffer[3] = 255u8
    buffer[4] = 228u8; buffer[5] = 184u8; buffer[6] = 173u8; buffer[7] = 10u8
    let result = io.write_all(1, &mut buffer, 8)
    if mode == 48 { if result != 0 { return 1 } }
    else { if result != -1 { return 2 }; io.emit(69) }
    return 0
}
