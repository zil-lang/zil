extern "C" fn putchar(code:i32)->i32;
resource Token {code:i32}
impl Drop for Token {fn drop(self:Self){unsafe{putchar(.code);}}}
fn child(code:i32)->Token contract {result(create); effects(suspend); resume(local)} {
    let result=Token{code:code};
    task_yield();
    task_yield();
    return give result;
}
fn parent(code:i32)->Token contract {result(create); effects(suspend); resume(local)} {
    let result=task_await(child,code);
    task_yield();
    return give result;
}
fn drive(task:&mut Task<Managed<Token>>)->std.task.TaskPollState!i32 {
    match task_poll(task)? {
        std.task.Poll.Pending()=>{unsafe{putchar(80);} return 0;},
        std.task.Poll.Ready(value)=>{unsafe{putchar(82);} return 0;}
    }
}
fn main()->std.task.TaskCreateError!i32 {
    var task=task_create(parent,65)?;
    drive(&mut task);drive(&mut task);drive(&mut task);drive(&mut task);drive(&mut task);
    task_request_cancel(&mut task);
    task_poll_close(&mut task);
    task_finish_destroy(give task);
    return 0;
}
