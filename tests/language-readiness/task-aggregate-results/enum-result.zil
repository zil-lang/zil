extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code); } } }
enum Packet { Empty, Item(manages Token) }
fn worker(give token:Token)->Packet {
    let packet=Packet.Item(give token);
    task_yield();
    return packet;
}
fn drive(task:&mut Task<Packet>)->TaskPollState!i32 {
    match task_poll(task)? {
        std.task.Poll.Pending()=>{unsafe{putchar(80);}return 0;},
        std.task.Poll.Ready(packet)=>{
            match packet {
                Packet.Empty()=>{return 1;},
                Packet.Item(value)=>{unsafe{putchar(82);}return value.code;}
            }
        }
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
