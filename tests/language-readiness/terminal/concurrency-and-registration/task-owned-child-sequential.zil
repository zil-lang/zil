extern "C" fn putchar(code:i32)->i32;
resource Token {code:i32}
impl Drop for Token {fn drop(self:Self){unsafe{putchar(.code);}}}
fn child(give token:Token)->Token contract {result(give); effects(suspend); resume(local)} {
    task_yield();
    task_yield();
    return give token;
}
fn parent(give a:Token,give b:Token)->i32 contract {effects(suspend); resume(local)} {
    let first=task_await(child,give a);
    let second=task_await(child,give b);
    return 0;
}
fn main()->std.task.TaskCreateError!i32 {
    let a=Token{code:65}; let b=Token{code:66};
    var task=task_create(parent,give a,give b)?;
    // POLLS
    task_request_cancel(&mut task);
    task_poll_close(&mut task);
    task_finish_destroy(give task);
    return 0;
}
