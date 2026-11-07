extern "C" fn putchar(code:i32)->i32;
fn worker(value:bool)->bool{task_yield();return value;}
fn drive(task:&mut Task<bool>)->std.task.TaskPollState!i32{
    match task_poll(task)?{
        std.task.Poll.Pending()=>{unsafe{putchar(80);}return 0;},
        std.task.Poll.Ready(value)=>{if value {unsafe{putchar(84);}} else {unsafe{putchar(70);}} return 0;}
    }
}
fn main()->std.task.TaskCreateError!i32{
    var task=task_create(worker,true)?;drive(&mut task);drive(&mut task);drive(&mut task);
    task_poll_close(&mut task);task_finish_destroy(give task);return 0;
}
