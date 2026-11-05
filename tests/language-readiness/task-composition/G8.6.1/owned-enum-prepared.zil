extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code); } } }
enum Inner { Empty, Item(manages Token) }
enum Outer { Empty, Pair(Inner,i32) }
fn child(n:i32)->i32 { task_yield(); return n; }
fn worker(give token:Token)->i32 {
    // The complete Inner SSA value owns A while the later argument suspends.
    let packet=Outer.Pair(Inner.Item(give token),task_await(child,42));
    task_yield();
    match packet {
        Outer.Empty()=>{return 0;},
        Outer.Pair(Inner.Empty(),_)=>{return 1;},
        Outer.Pair(Inner.Item(value),n)=>{return value.code*100+n;}
    }
}
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
    drive(&mut task);
    task_request_cancel(&mut task);
    task_poll_close(&mut task);
    task_finish_destroy(give task);
    return 0;
}
