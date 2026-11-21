extern "C" fn thread_register(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),thread_async_callback_register(),thread_async_driver_transfer());effects(ffi);};
extern "C" fn thread_request(token:c_handle) contract{provides(static_callbacks(),thread_async_callback_request_close(),thread_async_driver_transfer());effects(ffi);};
extern "C" fn thread_drain(token:c_handle)->i32 contract{provides(static_callbacks(),thread_async_callback_poll_drain(),thread_async_driver_transfer());effects(ffi);};
extern "C" fn thread_parent()->i32;
extern "C" fn thread_complete(code:i32)->i32;
fn compute(seed:i32)->i32 contract{resume(transfer)} {
    let bytes=bytes_new();bytes_reserve(bytes,4);bytes_push(bytes,37u8);
    var count=seed;
    let callback=move(value:i32)->i32=>{count+=1;return value+count+i32(bytes_len(bytes))-1};
    unsafe {
        var owner=c_register_async(thread_register,thread_request,thread_drain,callback);
        task_yield();
        async_registration_request_close(&mut owner);
        loop {match async_registration_poll_close(&mut owner){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
        async_registration_finish_destroy_checked(give owner);
    }
    return seed+5;
}
fn drive(give input:Task<i32>)->i32 {
    var task=give input;var answer=0;
    loop {match task_poll(&mut task){.ok(p)=>{match p{Poll.Pending()=>{continue;},Poll.Ready(v)=>{answer=v;break;}}},.err(_)=>{break;}}}
    task_request_cancel(&mut task);
    loop {match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
    task_finish_destroy(give task);return answer;
}
fn main()->TaskCreateError!i32 {
    var task=task_create(compute,37)?;
    task_poll(&mut task);
    match thread_scope(drive,give task,()=>{unsafe{return thread_parent()}}) {
        TaskThreadScopeResult.Failed(input)=>{
            var remaining=give input;
            task_request_cancel(&mut remaining);
            loop {match task_poll_close(&mut remaining){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
            task_finish_destroy(give remaining);unsafe{return thread_complete(2)}
        },
        TaskThreadScopeResult.Completed(answer,body)=>{unsafe{return thread_complete(answer+body-51)}}
    }
}
