extern "C" fn putchar(code:i32)->i32;
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code);}}}
fn child(give token:Token,unit:void) contract{effects(suspend);resume(local)}{
    task_yield();task_yield();
}
fn parent(give a:Token,give b:Token) contract{effects(suspend);resume(local)}{
    task_await(child,give a,());task_yield();
}
fn main()->std.task.TaskCreateError!i32{
    let a=Token{code:65};let b=Token{code:66};var task=task_create(parent,give a,give b)?;
    // POLLS
    task_request_cancel(&mut task);task_poll_close(&mut task);task_poll_close(&mut task);task_finish_destroy(give task);return 0;
}
