import events
extern "C" fn getchar() -> i32;
extern "C" fn putchar(code: i32) -> i32;

fn main() -> i32 {
    var mode = 0
    unsafe { mode = getchar() }
    if mode == 48 {
        let code = events.pointer_event()
        unsafe { putchar(code) }
        return 0
    }
    if mode == 49 {
        let code = events.touch_event()
        unsafe { putchar(code) }
        return 0
    }
    return 7
}
