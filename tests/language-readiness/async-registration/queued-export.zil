pub fn worker(seed:i32)->i32 {
    let mark=Mark{code:37};
    var count=seed;
    let callback=move(value:i32)->i32=>{count+=1; unsafe {if async_reenter()!=0 {return -100}} return value+count+mark.code-37};
    unsafe {
    var owner=c_register_async(async_register,async_request,async_drain,callback);
    if !async_registration_valid(owner) {return 13}
    task_yield();
    return 17
    }
}
extern "C" fn async_register(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),async_callback_register());effects(ffi);};
extern "C" fn async_request(token:c_handle) contract{provides(static_callbacks(),async_callback_request_close());effects(ffi);};
extern "C" fn async_drain(token:c_handle)->i32 contract{provides(static_callbacks(),async_callback_poll_drain());effects(ffi);};
extern "C" fn async_pump()->i32 contract{effects(ffi);};
extern "C" fn async_reenter()->i32 contract{effects(ffi);};
extern "C" fn async_complete()->i32 contract{effects(ffi);};
extern "C" fn async_note(code:i32) contract{effects(ffi);};
resource Mark {code:i32}
impl Drop for Mark {fn drop(self:Self){unsafe{async_note(.code);}}}
extern "C" fn async_external()->i32 contract{effects(ffi);};
fn main()->i32 {unsafe{return async_external()}}
