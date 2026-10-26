mod io;
unsafe extern "C" {
    fn getchar() -> i32;
}
fn application() -> i32 {
    let mode = unsafe { getchar() };
    let mut buffer = io::empty();
    buffer[..8].copy_from_slice(&[0, 127, 128, 255, 228, 184, 173, 10]);
    let result = io::write_all(1, &mut buffer, 8);
    if mode == 48 {
        if result != 0 {
            return 1;
        }
    } else {
        if result != -1 {
            return 2;
        }
        io::emit(69);
    }
    0
}
fn main() {
    std::process::exit(application());
}
