extern "C" fn registration_register(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn registration_close_backend(token:c_handle) contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
fn worker(base:i32)->i32 contract{effects(suspend);resume(local)} {
    unsafe {
        let owner=c_register(registration_register,registration_close_backend,move(value:i32)->i32=>value+base);
        task_yield();
        registration_close(give owner);
    }
    return 42;
}
fn main()->std.task.TaskCreateError!i32 {
    var task=task_create(worker,0)?;
    task_request_cancel(&mut task);
    task_poll_close(&mut task);
    task_finish_destroy(give task);
    return 0;
}
