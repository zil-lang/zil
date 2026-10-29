import storage
import work
extern "C" fn getchar() -> i32;

fn main() -> void!i32 {
    var count: i32 = 0
    unsafe { count = getchar() - 48 }
    if count < 0 || count > 9 { return () }
    let value = storage.empty()
    work.fill(value, count)
    if store_len(value) != i64(count) { return () }
    let moved = storage.relay(give value)
    let sum = work.drain(moved)?
    return sum - (count * 65 + count * (count - 1) / 2)
}
