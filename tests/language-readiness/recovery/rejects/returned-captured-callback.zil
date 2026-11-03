extern "C" fn register(callback: extern "C" fn(c_handle, i32) -> i64, context: c_handle) -> c_handle
contract { provides(static_callbacks()); effects(ffi); };
fn install(seed: i32) -> c_handle {
    let callback = move(context: c_handle, value: i32) -> i64 => i64(value + seed)
    unsafe { return register(c_callback(callback), c_null()) }
}
fn main() -> i32 { let token = install(7); return 0 }
