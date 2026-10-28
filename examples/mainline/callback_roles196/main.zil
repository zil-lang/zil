import factory
import helpers
import portable

fn main() -> i32 {
    let input = portable.read()
    if input < 48 || input > 55 { return 9 }
    let count = input - 48
    var values = [65u8, 97u8]
    var first = factory.make(65)
    var second = factory.make(66)
    let response = helpers.drive(
        &mut first, &mut second, &mut values[..], count,
        portable.Tag { sequence: i64(count), valid: true }
    )
    if response.1.sequence != i64(count) || !response.1.valid { return 8 }
    if response.0 != 0 { return response.0 }
    var rounds = 0
    if count > 0 { rounds += 1 }
    if count > 2 { rounds += 1 }
    if count > 3 { rounds += 1 }
    if values[0] != u8(65 + 4 * rounds) || values[1] != u8(97 + 4 * rounds) {
        return 8
    }
    return 0
}
