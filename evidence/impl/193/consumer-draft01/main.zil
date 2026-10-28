import factory
import helpers
import portable
extern "C" fn getchar() -> i32;

fn main() -> i32 {
    let checked = portable.check()
    if checked != 0 { return checked }
    var input = 0
    unsafe { input = getchar() }
    if input < 48 || input > 51 { return 9 }
    var values = [65u8, 97u8]
    var callback = factory.make()
    helpers.emit(i32(values[0]))
    var count = 0
    while count < input - 48 {
        let result = helpers.edit(&mut callback, &mut values[..])
        if result < 0 { return 1 }
        if result != count + 1 || values[1] != u8(98 + count) { return 8 }
        helpers.emit(i32(values[0]))
        count += 1
    }
    return 0
}
