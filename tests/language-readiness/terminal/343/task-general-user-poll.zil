enum Poll<T> { Pending, Ready(T) }
fn read_user(value:Poll<i32>)->i32 {
    match value { Poll.Pending()=>{return 0;},Poll.Ready(n)=>{return n;} }
}
fn shadow(Poll:i32)->i32 { return Poll; }
extern "C" fn putchar(code:i32)->i32;
fn worker(x:i32)->i32 { task_yield(); return x }
fn drive(task:&mut Task<i32>)->std.task.TaskPollState!i32 {
    let outcome = task_poll(task)?;
    match outcome {
        std.task.Poll.Pending() => { unsafe { putchar(80); } return 0; },
        std.task.Poll.Ready(value) => { unsafe { putchar(value); } return value; }
    }
}
fn main()->std.task.TaskCreateError!i32 {
    let unrelated:Poll<i32> = Poll.Ready(7);
    if read_user(unrelated) != 7 || shadow(7) != 7 {return 4;}
    var task = task_create(worker, 82)?;
    drive(&mut task);
    drive(&mut task);
    drive(&mut task);
    match task_poll_close(&mut task) {
        std.task.Poll.Pending() => { return 1; },
        std.task.Poll.Ready(_) => { unsafe { putchar(67); } }
    }
    task_finish_destroy(give task);
    return 0;
}
