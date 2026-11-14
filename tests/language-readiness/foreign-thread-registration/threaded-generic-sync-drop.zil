extern "C" fn thread_register(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),thread_async_callback_register());effects(ffi);};
extern "C" fn thread_request(token:c_handle) contract{provides(static_callbacks(),thread_async_callback_request_close());effects(ffi);};
extern "C" fn thread_drain(token:c_handle)->i32 contract{provides(static_callbacks(),thread_async_callback_poll_drain());effects(ffi);};
extern "C" fn thread_race()->i32 contract{effects(ffi);};
extern "C" fn thread_release()->i32 contract{effects(ffi);};
extern "C" fn thread_complete(failed:i32)->i32 contract{effects(ffi);};
fn install<F:FnMut(i32)->i32 + ClosedEnvironment + Transfer + ThreadCallable + SyncDrop>(f:F)->AsyncRegistration<F> contract{result(create);} {
    unsafe {return c_register_async(thread_register,thread_request,thread_drain,f)}
}
fn closed<F:FnMut(i32)->i32 + ClosedEnvironment>(owner:&mut AsyncRegistration<F>)->std.task.Poll<()> contract{requires(state(owner,Closing,Closed));provides(on_pending(state(owner,Closing)),on_ready(state(owner,Closed)));} {
    return async_registration_poll_close(owner)
}
package Payload {manages bytes:ByteBuffer;pub code:i32}
fn main()->i32 {
    let data=bytes_new();
    bytes_reserve(data,4);
    bytes_push(data,37u8);
    let payload=Payload{bytes:give data,code:0};
    var count=37;
    let callback=move(value:i32)->i32=>{count+=1;return value+count+i32(bytes_len(payload.bytes))-1+payload.code};
    unsafe {
    var owner=install(callback);
    if !async_registration_valid(owner) {
        async_registration_finish_destroy(give owner);
        unsafe{return thread_complete(1)}
    }
    unsafe {thread_race();}
    async_registration_request_close(&mut owner);
    async_registration_request_close(&mut owner);
    match closed(&mut owner) {
        std.task.Poll.Ready(_)=>{async_registration_finish_destroy(give owner);return 10;},
        std.task.Poll.Pending()=>{}
    }
    unsafe {thread_release();}
    loop {
        match closed(&mut owner) {
            std.task.Poll.Pending()=>{continue;},
            std.task.Poll.Ready(_)=>{break;}
        }
    }
    async_registration_request_close(&mut owner);
    async_registration_finish_destroy_checked(give owner);
    unsafe{return thread_complete(0)}
}
}
