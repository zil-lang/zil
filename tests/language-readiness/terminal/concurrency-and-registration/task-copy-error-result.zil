extern "C" fn putchar(code:i32)->i32;
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code);}}}
fn failed(stop:bool)->std.task.TaskPollState!i32{
    if stop {return std.task.TaskPollState.Invalid;}
    return 42;
}
fn child(give token:Token,value:i32)->i32 contract{effects(suspend);resume(local)}{
    task_yield();task_yield();return value;
}
fn parent(give a:Token,give b:Token,stop:bool)->std.task.TaskPollState!i32 contract{effects(suspend);resume(local)}{
    return task_await(child,give a,failed(stop)?);
}
fn drive(task:&mut Task<std.task.TaskPollState!i32>)->std.task.TaskPollState!i32{
    match task_poll(task)? {
        std.task.Poll.Pending()=>{unsafe{putchar(80);}return 0;},
        std.task.Poll.Ready(value)=>{let result=value?;unsafe{putchar(result+40);}return 0;}
    }
}
fn main()->std.task.TaskCreateError!i32{
    let a=Token{code:65};let b=Token{code:66};var task=task_create(parent,give a,give b,false)?;
    drive(&mut task);drive(&mut task);drive(&mut task);drive(&mut task);
    task_request_cancel(&mut task);task_poll_close(&mut task);task_finish_destroy(give task);return 0;
}
