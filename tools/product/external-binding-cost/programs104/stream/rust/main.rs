mod io;
mod transform;
fn run() -> i32 {
    let mut input = io::read_byte();
    while input >= 0 {
        io::write_byte(transform::upper(input as u8));
        input = io::read_byte();
    }
    0
}
fn main() { let code = run(); if code != 0 { std::process::exit(code); } }
