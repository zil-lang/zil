extern "C" fn register(callback: extern "C" fn(c_handle, i32) -> i64, context: c_handle) -> c_handle
contract { provides(static_callbacks(), scoped_callback_register()); effects(ffi); };
extern "C" fn close(token: c_handle)
contract { provides(static_callbacks(), scoped_callback_close()); effects(ffi); };
fn main() -> i32 {
    let seed = 7
    var callback = move(value: i32) -> i32 => value + seed
    let body = () -> i32 => 0
    unsafe {
        let result = c_with_callback(register, close, &mut callback, body)
        if !result.0 { return 1 }
        return callback(0) - 7
    }
}
