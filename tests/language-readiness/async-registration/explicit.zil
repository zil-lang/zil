extern "C" fn async_register(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),async_callback_register());effects(ffi);};
extern "C" fn async_request(token:c_handle) contract{provides(static_callbacks(),async_callback_request_close());effects(ffi);};
extern "C" fn async_drain(token:c_handle)->i32 contract{provides(static_callbacks(),async_callback_poll_drain());effects(ffi);};
fn main()->i32 {
    var count=37;
    let callback=move(value:i32)->i32=>{count+=1;return value+count};
    unsafe {
        var owner=c_register_async(async_register,async_request,async_drain,callback);
        async_registration_valid(owner);
        async_registration_request_close(&mut owner);
        async_registration_poll_close(&mut owner);
        async_registration_finish_destroy(give owner);
    }
    return 0
}
