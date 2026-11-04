extern "C" fn async_register(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),async_callback_register());effects(ffi);};
extern "C" fn async_request(token:c_handle) contract{provides(static_callbacks(),async_callback_request_close());effects(ffi);};
extern "C" fn async_drain(token:c_handle)->i32 contract{provides(static_callbacks(),async_callback_poll_drain());effects(ffi);};
fn worker(seed:i32)->i32 {
    var count=seed;
    let callback=move(value:i32)->i32=>{count+=1; return value+count};
    var owner=unsafe {c_register_async(async_register,async_request,async_drain,callback)};
    task_yield();
    async_registration_request_close(&mut owner);
    async_registration_poll_close(&mut owner);
    async_registration_finish_destroy(give owner);
    return 0
}
fn main()->std.task.TaskCreateError!i32 {
    var task=task_create(worker,37)?;
    task_poll(&mut task);
    task_request_cancel(&mut task);
    task_poll_close(&mut task);
    task_finish_destroy(give task);
    return 0
}
