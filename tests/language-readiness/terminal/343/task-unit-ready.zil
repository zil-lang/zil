extern "C" fn putchar(code:i32)->i32;
fn child(){}
fn worker() contract{effects(suspend);resume(local)}{task_await(child);task_yield();}
fn drive(task:&mut Task<void>)->std.task.TaskPollState!i32{
    match task_poll(task)?{
        std.task.Poll.Pending()=>{unsafe{putchar(80);}return 0;},
        std.task.Poll.Ready(unit)=>{unsafe{putchar(82);}return 0;}
    }
}
fn main()->std.task.TaskCreateError!i32{
    child();var task=task_create(worker)?;drive(&mut task);drive(&mut task);drive(&mut task);
    task_poll_close(&mut task);task_finish_destroy(give task);return 0;
}
