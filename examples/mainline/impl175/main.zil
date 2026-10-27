import model
import storage
import inspection

extern "C" fn getchar() -> i32;
extern "C" fn putchar(value: i32) -> i32;

fn main() -> model.Fault!i32 {
    var count: i32 = 0
    var index: i32 = 0
    unsafe { count = getchar() - 48; index = getchar() - 48 }
    if count < 0 || count > 4 || index < 0 || index >= count {
        if count == 0 && index == 0 { unsafe { putchar(45) }; return 0 }
        return 9
    }
    let items = storage.empty()
    storage.fill(items, count)?
    let view = store_at(items, i64(index))
    let code = inspection.code(view)
    let expected = count * 65 + count * (count - 1) / 2
    if code != 65 + index || inspection.sum(items) != expected { return 90 }
    unsafe { putchar(code + 32) }
    // All uses of the original window ended before reserve.
    if !store_reserve(items, i64(count + 2)) {
        if inspection.sum(items) != expected { return 91 }
        return model.Fault.Memory
    }
    if inspection.code(store_at(items, i64(index))) != code || inspection.sum(items) != expected {
        return 92
    }
    return 0
}
