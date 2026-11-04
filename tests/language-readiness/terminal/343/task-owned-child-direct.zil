extern "C" fn putchar(code:i32)->i32;
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code);}}}
fn child(give token:Token)->Token contract{result(give)}{return give token;}
fn parent(give token:Token)->Token contract{result(give);effects(suspend);resume(local)}{
    let result=task_await(child,give token);task_yield();return give result;
}
fn main()->std.task.TaskCreateError!i32{
    let input=Token{code:65};let ordinary=child(give input);
    var task=task_create(parent,Token{code:66})?;
    task_poll(&mut task);task_poll(&mut task);
    task_request_cancel(&mut task);task_poll_close(&mut task);task_finish_destroy(give task);
    return 0;
}
