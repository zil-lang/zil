import model
import storage
import edit

extern "C" fn getchar() -> i32;

fn main() -> model.Fault!i32 {
    var count: i32 = 0
    unsafe { count = getchar() - 48 }
    if count < 0 || count > 4 { return 9 }
    let items = storage.empty()
    storage.fill(items, count)?
    let expected = count * 97 + count * (count - 1) / 2
    if edit.apply(items, 32) != expected { return 90 }
    var index: i64 = 0
    while index < i64(count) {
        model.emit(edit.code(store_at(items, index)))
        index += 1
    }
    // No element address survives the update/inspection helpers.
    if !store_reserve(items, i64(count + 3)) { return model.Fault.Memory }
    if count > 0 && edit.code(store_at(items, i64(count - 1))) != 96 + count {
        return 91
    }
    return 0
}
