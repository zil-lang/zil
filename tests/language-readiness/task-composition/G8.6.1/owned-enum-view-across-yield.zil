extern "C" fn putchar(code:i32)->i32;
extern "C" fn getchar()->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code); } } }
enum Packet { Left(manages Token), Right(manages Token) }
fn worker(flag:bool,give token:Token)->i32 {
    let packet=if flag{Packet.Left(give token)}else{Packet.Right(give token)};
    match packet {
        Packet.Left(ref left)=>{task_yield();unsafe{putchar(81);}return left.code+100;},
        Packet.Right(ref right)=>{task_yield();unsafe{putchar(81);}return right.code+200;}
    }
}
fn drive(task:&mut Task<i32>)->TaskPollState!i32 {
    match task_poll(task)? {
        std.task.Poll.Pending()=>{unsafe{putchar(80);}return 0;},
        std.task.Poll.Ready(value)=>{unsafe{putchar(82);}return value;}
    }
}
fn main()->TaskCreateError!i32 {
    var selector=0;
    unsafe{selector=getchar();}
    let flag=selector==76;
    let answer=if flag{165}else{265};
    let input=Token{code:65};
    var task=task_create(worker,flag,give input)?;
    match drive(&mut task){.ok(value)=>{if value!=0{unsafe{putchar(88);}return 91;}},.err(_)=>{return 92;}}
    match drive(&mut task){.ok(value)=>{if value!=answer{unsafe{putchar(88);}return 91;}},.err(_)=>{return 92;}}
    task_request_cancel(&mut task);
    task_poll_close(&mut task);
    match task_poll(&mut task){.ok(_)=>{unsafe{putchar(88);}return 93;},.err(_)=>{}}
    task_finish_destroy(give task);
    return 0;
}
