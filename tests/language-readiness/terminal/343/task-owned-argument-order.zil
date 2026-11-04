extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code); } } }
fn child(value:i32)->i32 {task_yield();return value;}
fn combine(give token:Token,value:i32)->i32 {return value;}
fn worker(give a:Token,give b:Token)->i32 contract {effects(suspend);resume(local)} {
    return combine(give a,task_await(child,0));
}
fn main()->std.task.TaskCreateError!i32 {
    let a=Token{code:65};
    let b=Token{code:66};
    var task=task_create(worker,give a,give b)?;
    task_poll(&mut task);
    task_request_cancel(&mut task);
    task_poll_close(&mut task);
    task_finish_destroy(give task);
    return 0;
}
