extern "C" fn child_observe(code:i32) contract {effects(ffi);};
extern "C" fn child_close_complete()->i32 contract {effects(ffi);};
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{child_observe(.code);}}}
enum Packet { Empty, Live(manages Mark,manages Task<i32>,manages Mark) }
fn child()->i32 { unsafe{child_observe(88);} task_yield(); return 0; }
enum Stopped { Here }
fn worker()->Stopped!i32 {
    match task_create(child) {
        .err(_)=>{return 0;}
        .ok(task)=>{
            let before=Mark{code:65};
            let after=Mark{code:66};
            let packet=Packet.Live(give before,give task,give after);
            match packet { Packet.Live(extracted,_,ref observer)=>{let seen=observer.code;}, Packet.Empty()=>{} }
            task_yield();
            return Stopped.Here;
        }
    }
}
fn main()->std.task.TaskCreateError!i32 {
    var task=task_create(worker)?;
    loop {
        match task_poll(&mut task) {
            .err(_)=>{return 90;}
            .ok(poll)=>{match poll {
                Poll.Pending()=>{continue;}
                Poll.Ready(value)=>{match value {.ok(_)=>{return 91;},.err(_)=>{break;}}}
            }}
        }
    }
    loop {
        match task_poll_close(&mut task) {
            Poll.Pending()=>{continue;}
            Poll.Ready(_)=>{break;}
        }
    }
    task_finish_destroy(give task);
    unsafe{return child_close_complete();}
}
