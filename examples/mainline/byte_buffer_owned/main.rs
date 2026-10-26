use std::io::{Read, Write};
fn make() -> impl FnMut(u8) -> u8 {
    let mut data = Vec::new();
    let mut sum = 0i64;
    move |byte| {
        if byte == 0 {
            data = Vec::new();
            sum = 0;
            return 0;
        }
        data.push(byte);
        sum += i64::from(byte);
        ((sum + data.len() as i64) % 251) as u8
    }
}
fn main() {
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input).unwrap();
    let mut update = make();
    let output: Vec<u8> = input.into_iter().map(&mut update).collect();
    std::io::stdout().write_all(&output).unwrap();
}
