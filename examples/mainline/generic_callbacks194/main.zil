import factory
import helpers
import portable
extern "C" fn getchar() -> i32;

fn main() -> i32 {
    let checked = portable.check()
    if checked != 0 { return checked }
    var input = 0
    unsafe { input = getchar() }
    if input < 48 || input > 55 { return 9 }
    var values = [65u8, 97u8]
    var callback = factory.make()
    helpers.emit(i32(values[0]))
    var count = 0
    while count < input - 48 {
        var observed = 0
        if count % 2 == 0 {
            let response = helpers.stamp(&mut callback, count, &mut values[..])
            if response.1 != count { return 8 }
            observed = response.0
        } else {
            let response = helpers.stamp(
                &mut callback, helpers.Tag { sequence: i64(count), valid: true },
                &mut values[..]
            )
            if response.1.sequence != i64(count) || !response.1.valid { return 8 }
            observed = response.0
        }
        if observed < 0 { return 1 }
        if observed != count + 1 || values[1] != u8(98 + count) { return 8 }
        helpers.emit(i32(values[0]))
        count += 1
    }
    return 0
}
