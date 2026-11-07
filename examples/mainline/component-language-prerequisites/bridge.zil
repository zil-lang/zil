extern "C" fn component_register(callback:extern "C" fn(c_handle,i32)->i64,data:c_handle)->c_handle
contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn component_close(token:c_handle)
contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
extern "C" fn component_pump(value:i32)->i32
contract{provides(static_callbacks());effects(ffi);};
extern "C" fn component_mode()->i32;
extern "C" fn component_drop(code:i32);
extern "C" fn component_after();
extern "C" fn putchar(code:i32)->i32;

pub fn emit(code:i32) { unsafe { putchar(code) } }
pub fn dropped(code:i32) { unsafe { component_drop(code) } }
pub fn mode()->i32 { unsafe { return component_mode() } }
pub fn after() { unsafe { component_after() } }
pub fn pump(value:i32)->i32 { unsafe { return component_pump(value) } }
pub fn run<F:FnMut(i32)->i32,B:FnOnce()->i32>(callback:&mut F,body:B)->(bool,i32) {
    unsafe { return c_with_callback(component_register,component_close,callback,body) }
}
