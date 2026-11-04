extern "C" fn putchar(code:i32)->i32;
resource Token {code:i32,p0:i32,p1:i32,p2:i32,p3:i32,p4:i32,p5:i32,p6:i32,p7:i32}
impl Drop for Token {fn drop(self:Self){unsafe{putchar(.code);}}}
fn child(give token:Token)->Token contract {result(give); effects(suspend); resume(local)} {
    task_yield();
    task_yield();
    return give token;
}
fn parent(give token:Token)->Token contract {result(give); effects(suspend); resume(local)} {
    let result=task_await(child,give token);
    task_yield();
    return give result;
}
fn main()->std.task.TaskCreateError!i32 {
    let input=Token{code:65,p0:0,p1:0,p2:0,p3:0,p4:0,p5:0,p6:0,p7:0};
    var task=task_create(parent,give input)?;
    // POLLS
    task_request_cancel(&mut task);
    task_poll_close(&mut task);
    task_finish_destroy(give task);
    return 0;
}
