extern "C" fn putchar(code:i32)->i32;
fn child(value:i32)->(bool,i32){task_yield();return (true,value);}
fn worker(value:i32)->(bool,i32) contract{effects(suspend);resume(local)}{return task_await(child,value);}
fn drive(task:&mut Task<(bool,i32)>)->std.task.TaskPollState!i32{
    match task_poll(task)?{
        std.task.Poll.Pending()=>{unsafe{putchar(80);}return 0;},
        std.task.Poll.Ready(value)=>{if value.0 {unsafe{putchar(value.1+40);}} return 0;}
    }
}
fn main()->std.task.TaskCreateError!i32{
    var task=task_create(worker,42)?;drive(&mut task);drive(&mut task);drive(&mut task);
    task_poll_close(&mut task);task_finish_destroy(give task);return 0;
}
