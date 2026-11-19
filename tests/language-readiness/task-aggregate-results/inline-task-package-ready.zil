extern "C" fn child_observe(code:i32) contract {effects(ffi);};
extern "C" fn child_close_complete()->i32 contract {effects(ffi);};
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{child_observe(.code);}}}
package Packet {manages first:Mark; manages child:Task<i32>; manages last:Mark}
fn child()->i32 { unsafe{child_observe(88);} task_yield(); return 0; }
fn worker()->i32 {
    match task_create(child) {
        .err(_)=>{return 0;}
        .ok(task)=>{
            let before=Mark{code:65};
            let after=Mark{code:66};
            let packet=Packet{first:give before,child:give task,last:give after};
            task_yield();
            return 0;
        }
    }
}
fn main()->std.task.TaskCreateError!i32 {
    var task=task_create(worker)?;
    match task_poll(&mut task) {
        .err(_)=>{return 90;}
        .ok(poll)=>{match poll {
            Poll.Pending()=>{}
            Poll.Ready(_)=>{}
        }}
    }
    match task_poll(&mut task) { .err(_)=>{return 92;}, .ok(_)=>{} }
    loop {
        match task_poll_close(&mut task) {
            Poll.Pending()=>{continue;}
            Poll.Ready(_)=>{break;}
        }
    }
    task_finish_destroy(give task);
    unsafe{return child_close_complete();}
}
