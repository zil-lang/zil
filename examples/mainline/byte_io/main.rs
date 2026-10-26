mod io;
fn application() -> i32 {
    let mut buffer = io::empty();
    loop {
        let count = io::read_chunk(0, &mut buffer);
        if count < 0 {
            return 1;
        }
        if count == 0 {
            return 0;
        }
        let mut index = 0;
        while index < count {
            let byte = buffer[index as usize];
            if (97..=122).contains(&byte) {
                buffer[index as usize] = byte - 32;
            }
            index += 1;
        }
        if io::write_all(1, &mut buffer, count) != 0 {
            return 2;
        }
    }
}
fn main() {
    std::process::exit(application());
}
