extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code); } } }
fn worker(give token:Token)->Token contract { result(give); effects(suspend); resume(local) } {
    task_yield();
    return give token;
}
fn drive(task:&mut Task<Managed<Token>>)->std.task.TaskPollState!i32 {
    match task_poll(task)? {
        std.task.Poll.Pending()=>{unsafe{putchar(80);} return 0;},
        std.task.Poll.Ready(value)=>{unsafe{putchar(82);} return 0;}
    }
}
fn main()->std.task.TaskCreateError!i32 {
    let input=Token{code:65};
    var task=task_create(worker,give input)?;
    drive(&mut task);
    drive(&mut task);
    task_request_cancel(&mut task);
    task_poll_close(&mut task);
    task_finish_destroy(give task);
    return 0;
}
