extern "C" fn async_register(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),async_callback_register());effects(ffi);};
extern "C" fn async_request(token:c_handle) contract{provides(static_callbacks(),async_callback_request_close());effects(ffi);};
extern "C" fn async_drain(token:c_handle)->i32 contract{provides(static_callbacks(),async_callback_poll_drain());effects(ffi);};
extern "C" fn async_pump()->i32 contract{effects(ffi);};
extern "C" fn async_reenter()->i32 contract{effects(ffi);};
extern "C" fn async_complete()->i32 contract{effects(ffi);};
extern "C" fn async_note(code:i32) contract{effects(ffi);};
resource Mark {code:i32}
impl Drop for Mark {fn drop(self:Self){unsafe{async_note(.code);}}}
extern "C" fn async_result_drop(code:i32) contract{effects(ffi);};
resource Answer {code:i32}
impl Drop for Answer {fn drop(self:Self){unsafe{async_result_drop(.code);}}}
fn worker(seed:i32)->Answer contract{result(create);} {
    let mark=Mark{code:37};
    var count=seed;
    let callback=move(value:i32)->i32=>{count+=1; unsafe {if async_reenter()!=0 {return -100}} return value+count+mark.code-37};
    unsafe {
    var owner=c_register_async(async_register,async_request,async_drain,callback);
    if !async_registration_valid(owner) {let failure=Answer{code:13};return give failure}
    task_yield();
    let answer=Answer{code:17};
    return give answer
    }
}
extern "C" fn async_seen(value:i32) contract{effects(ffi);};
fn drive(task:&mut Task<Managed<Answer>>)->TaskPollState!i32 {
    match task_poll(task)? {
        std.task.Poll.Pending()=>{unsafe{async_seen(0);}return 0;},
        std.task.Poll.Ready(value)=>{unsafe{async_seen(value.code);}return value.code;}
    }
}
fn main()->std.task.TaskCreateError!i32 {
    var task=task_create(worker,37)?;
    drive(&mut task);
    drive(&mut task);
    unsafe {if async_pump()!=0 {task_finish_destroy(give task);return 3}}
    drive(&mut task);
    match task_poll_close(&mut task) {
        std.task.Poll.Pending()=>{task_finish_destroy(give task);return 4;},
        std.task.Poll.Ready(_)=>{}
    }
    task_finish_destroy(give task);
    unsafe {return async_complete()}
}
