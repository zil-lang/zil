// Same scalar libc boundary and stream/error contract as stdio.zil.
mod stdio {
    unsafe extern "C" {
        fn getchar() -> i32;
        fn putchar(value: i32) -> i32;
    }
    pub fn read() -> i32 {
        // No borrowed storage crosses the boundary; the linked libc obeys its
        // exact C signature and does not unwind into these Rust frames.
        unsafe { getchar() }
    }
    pub fn write(value: i32) -> bool {
        unsafe { putchar(value) >= 0 }
    }
}

fn run() -> u8 {
    let mut seen = false;
    loop {
        let value = stdio::read();
        if value < 0 { break; }
        seen = true;
        if (value < 32 && value != 9 && value != 10 && value != 13) || value > 126 {
            return 2;
        }
        let output = if (97..=122).contains(&value) { value - 32 } else { value };
        if !stdio::write(output) { return 4; }
    }
    if !seen { return 3; }
    0
}

fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(run())
}
