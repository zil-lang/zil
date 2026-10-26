import facade
import transform

extern "C" fn getchar() -> i32;
extern "C" fn putchar(value: i32) -> i32;

fn main() -> i32 {
    let prepared: facade.Table<i64> = #(facade.table())
    var value: i32 = -1
    unsafe { value = getchar() }
    if value < 0 { return 1 }
    let item = transform.wrapped(i64(value))
    let base = prepared.0.value * prepared.0.weight +
               prepared.1.value * prepared.1.weight
    let result = (transform.score(item) + base) % 256
    unsafe { if putchar(i32(result)) < 0 { return 2 } }
    return 0
}
