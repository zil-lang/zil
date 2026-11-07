extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code); } } }
enum Packet { Empty, Item(manages Token) }
fn child(give token:Token)->i32 {
    let packet=Packet.Item(give token);
    task_yield();
    match packet {
        Packet.Empty()=>{return 0;},
        Packet.Item(value)=>{return value.code;}
    }
}
fn worker(give token:Token)->i32 { return task_await(child,give token); }
fn drive(task:&mut Task<i32>)->TaskPollState!i32 {
    match task_poll(task)? {
        std.task.Poll.Pending()=>{unsafe{putchar(80);}return 0;},
        std.task.Poll.Ready(value)=>{unsafe{putchar(82);}return value;}
    }
}
fn main()->TaskCreateError!i32 {
    let input=Token{code:65};
    var task=task_create(worker,give input)?;
    drive(&mut task);
    drive(&mut task);
    task_request_cancel(&mut task);
    task_poll_close(&mut task);
    task_finish_destroy(give task);
    return 0;
}
