mod io;
fn sample(a: u8, b: u8) -> i64 {
    let mut total =
        i64::from(a.wrapping_add(b)) + i64::from(a.wrapping_sub(b)) + i64::from(a.wrapping_mul(b));
    total += i64::from(a & b) + i64::from(a | b) + i64::from(a ^ b) + i64::from(!a);
    if a < b {
        total += 1;
    }
    if a <= b {
        total += 2;
    }
    if a > b {
        total += 4;
    }
    if a >= b {
        total += 8;
    }
    if a == b {
        total += 16;
    }
    if a != b {
        total += 32;
    }
    if b != 0 {
        total += i64::from(a / b) + i64::from(a % b);
    }
    if b < 8 {
        total += i64::from(a << b) + i64::from(a >> b);
    }
    if i32::from(a) + i32::from(b) <= 255 {
        total += i64::from(a + b);
    }
    if a >= b {
        total += i64::from(a - b);
    }
    if i32::from(a) * i32::from(b) <= 255 {
        total += i64::from(a * b);
    }
    total
}
fn main() {
    let mut checksum = 0i64;
    for a in 0u16..256 {
        for b in 0u16..256 {
            checksum += sample(u8::try_from(a).unwrap(), u8::try_from(b).unwrap());
        }
    }
    const FOLDED: u8 = 250u8.wrapping_add(10);
    const CONVERTED: i64 = 255;
    if FOLDED != 4 || CONVERTED != 255 {
        std::process::exit(1);
    }
    io::number(checksum);
}
