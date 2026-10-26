resource Token { n: i32 }
fn write(r: Token) { r.n += 1 }
fn forward(r: Token) { write(r) }
fn main() -> i32 {
    let r = Token { n: 0 }
    let callback = () => { forward(r); r.n }
    return callback() + callback() - 3
}
