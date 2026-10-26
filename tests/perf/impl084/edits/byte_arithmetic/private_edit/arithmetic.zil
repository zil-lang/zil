import io
fn sample(a: u8, b: u8) -> i64 {
    var total = i64(a +% b) + i64(a -% b) + i64(a *% b)
    total += i64(a & b) + i64(a | b) + i64(a ^ b) + i64(~a)
    if a < b { total += 2 }
    if a <= b { total += 2 }
    if a > b { total += 4 }
    if a >= b { total += 8 }
    if a == b { total += 16 }
    if a != b { total += 32 }
    if b != 0u8 { total += i64(a / b) + i64(a % b) }
    if b < 8u8 { total += i64(a << b) + i64(a >> b) }
    if i32(a) + i32(b) <= 255 { total += i64(a + b) }
    if a >= b { total += i64(a - b) }
    if i32(a) * i32(b) <= 255 { total += i64(a * b) }
    return total
}
fn main() -> i32 {
    var checksum: i64 = 0
    for a in 0..256 { for b in 0..256 { checksum += sample(u8(a), u8(b)) } }
    let folded: u8 = #(u8(250) +% 10u8)
    let converted: i64 = #(i64(255u8))
    let zero: u8 = 0
    if folded != 4u8 || converted != 255 || -zero != 0u8 { return 1 }
    io.number(checksum)
    return 0
}
