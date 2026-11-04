extern "C" fn putchar(code:i32)->i32;
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code);}}}
fn child(give token:Token)->Token contract{result(give);effects(suspend);resume(local)}{
    task_yield();return give token;
}
fn parent()->i32 contract{effects(suspend);resume(local)}{
    var token=Token{code:65};var count=0;
    while count<3 {token=task_await(child,give token);count=count+1;}
    return 0;
}
fn main()->std.task.TaskCreateError!i32{
    var task=task_create(parent)?;
    // POLLS
    task_request_cancel(&mut task);task_poll_close(&mut task);task_finish_destroy(give task);
    return 0;
}
