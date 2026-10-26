import model
import transform
extern "C" fn getchar() -> i32;
extern "C" fn putchar(value: i32) -> i32;

fn main() -> i32 {
    let plan = #(model.prepare())
    var code: i32 = 0
    unsafe { code = getchar() }
    if code < 0 { return 1 }
    let result = transform.run(plan, u8(code))
    unsafe { if putchar(i32(result)) < 0 { return 2 } }
    return 0
}
