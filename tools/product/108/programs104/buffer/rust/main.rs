mod buffer;
mod sequence;
mod io;
fn run() -> i32 {
    let mut input = buffer::create();
    let mut code = io::read_byte();
    while code >= 0 {
        if !buffer::append(&mut input, code as u8) { return 2; }
        code = io::read_byte();
    }
    let mut ready = input;
    sequence::uppercase(&mut ready);
    for &byte in &ready { io::write_byte(byte); }
    0
}
fn main() { let code = run(); if code != 0 { std::process::exit(code); } }
