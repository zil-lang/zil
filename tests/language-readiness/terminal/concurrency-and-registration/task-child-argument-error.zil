extern "C" fn putchar(code:i32)->i32;
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code);}}}
enum Fault{Stop}
fn failed()->Fault!i32{return Fault.Stop;}
fn child(give token:Token,value:i32)->i32 contract{effects(suspend);resume(local)}{
    task_yield();return value;
}
fn parent(give a:Token,give b:Token)->Fault!i32 contract{effects(suspend);resume(local)}{
    return task_await(child,give a,failed()?);
}
fn main()->std.task.TaskCreateError!i32{
    let a=Token{code:65};let b=Token{code:66};var task=task_create(parent,give a,give b)?;
    task_poll(&mut task);task_poll(&mut task);task_request_cancel(&mut task);task_poll_close(&mut task);task_finish_destroy(give task);return 0;
}
