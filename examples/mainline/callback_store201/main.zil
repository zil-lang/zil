import factory
import state
import portable

fn main() -> void!i32 {
    let input = portable.read()
    if input < 48 || input > 55 { return 9 }
    let count = input - 48
    var values = [65u8, 97u8]
    let component = state.pack(
        factory.make(65), factory.make(66),
        portable.Tag { sequence: i64(count), valid: true }
    )
    let response = state.drive(component, &mut values[..], count)?
    if response.1.sequence != i64(count) || !response.1.valid { return 8 }
    if response.0 != 0 { return response.0 }
    var rounds = 0
    if count > 0 { rounds += 1 }
    if count > 2 { rounds += 1 }
    if count > 3 { rounds += 1 }
    var extra = 0
    if count > 0 { extra = 1 }
    if values[0] != u8(65 + 4 * rounds + extra) || values[1] != u8(97 + 4 * rounds + extra) {
        return 8
    }
    return 0
}
