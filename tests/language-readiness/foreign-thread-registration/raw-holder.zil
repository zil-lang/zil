extern "C" fn raw_register(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),async_callback_register());effects(ffi);};
extern "C" fn raw_request(token:c_handle) contract{provides(static_callbacks(),async_callback_request_close());effects(ffi);};
extern "C" fn raw_drain(token:c_handle)->i32 contract{provides(static_callbacks(),async_callback_poll_drain());effects(ffi);};
extern "C" fn raw_release()->i32 contract{effects(ffi);};
extern "C" fn raw_note(code:i32) contract{effects(ffi);};
extern "C" fn raw_complete()->i32 contract{effects(ffi);};
extern "C" fn raw_failure_complete()->i32 contract{effects(ffi);};
resource Mark {code:i32}
impl Drop for Mark {fn drop(self:Self){unsafe{raw_note(.code);}}}
fn main()->i32 {
    let mark=Mark{code:37};
    var count=37;
    let callback=move(value:i32)->i32=>{count+=1;return value+count+mark.code-37};
    unsafe {
        var owner=c_register_async(raw_register,raw_request,raw_drain,callback);
        if !async_registration_valid(owner) {
            async_registration_finish_destroy(give owner);
            return raw_failure_complete()
        }
        async_registration_request_close(&mut owner);
        match async_registration_poll_close(&mut owner) {
            std.task.Poll.Ready(_)=>{async_registration_finish_destroy(give owner);return 10;},
            std.task.Poll.Pending()=>{}
        }
        raw_release();
        loop {
            match async_registration_poll_close(&mut owner) {
                std.task.Poll.Pending()=>{continue;},
                std.task.Poll.Ready(_)=>{break;}
            }
        }
        async_registration_finish_destroy_checked(give owner);
        return raw_complete()
    }
}
