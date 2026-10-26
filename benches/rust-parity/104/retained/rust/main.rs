mod buffer;
mod state;
mod io;
fn run() -> i32 {
    let mut update = state::make();
    let mut input = io::read_byte();
    while input >= 0 {
        let result = update(input as u8);
        if result < 0 { return 2; }
        io::write_byte(result as u8);
        input = io::read_byte();
    }
    0
}
fn main() { let code = run(); if code != 0 { std::process::exit(code); } }
