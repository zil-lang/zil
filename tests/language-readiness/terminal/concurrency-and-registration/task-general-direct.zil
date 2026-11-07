extern "C" fn putchar(code:i32)->i32;
fn worker(x:i32)->i32 { return x }
fn drive(task:&mut Task<i32>)->TaskPollState!i32 {
    let outcome = task_poll(task)?;
    match outcome {
        Poll.Pending() => { unsafe { putchar(80); } return 0; },
        Poll.Ready(value) => { unsafe { putchar(value); } return value; }
    }
}
fn main()->TaskCreateError!i32 {
    var task = task_create(worker, 82)?;
    let direct = worker(82);
    if direct != 82 { return 2; }
    drive(&mut task);
    drive(&mut task);
    drive(&mut task);
    match task_poll_close(&mut task) {
        Poll.Pending() => { return 1; },
        Poll.Ready(_) => { unsafe { putchar(67); } }
    }
    task_finish_destroy(give task);
    return 0;
}
