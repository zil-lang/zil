extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code); } } }
enum Failure { Stopped }
fn worker(give token:Token,stop:bool)->Failure!Token contract { result(give); } {
    task_yield();
    if stop { return Failure.Stopped; }
    return give token;
}
fn drive(task:&mut Task<Failure!Managed<Token>>)->TaskPollState!i32 {
    match task_poll(task)? {
        std.task.Poll.Pending()=>{unsafe{putchar(80);}return 0;},
        std.task.Poll.Ready(result)=>{
            match result {
                .ok(value)=>{unsafe{putchar(82);}return value.code;},
                .err(_)=>{unsafe{putchar(69);}return 0;}
            }
        }
    }
}
fn main()->TaskCreateError!i32 {
    let input=Token{code:65};
    var task=task_create(worker,give input,false)?;
    drive(&mut task);
    drive(&mut task);
    task_request_cancel(&mut task);
    task_poll_close(&mut task);
    task_finish_destroy(give task);
    return 0;
}
