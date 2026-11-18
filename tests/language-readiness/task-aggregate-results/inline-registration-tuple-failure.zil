extern "C" fn async_register(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),async_callback_register());effects(ffi);};
extern "C" fn async_request(token:c_handle) contract{provides(static_callbacks(),async_callback_request_close());effects(ffi);};
extern "C" fn async_drain(token:c_handle)->i32 contract{provides(static_callbacks(),async_callback_poll_drain());effects(ffi);};
extern "C" fn async_pump()->i32 contract{effects(ffi);};
extern "C" fn async_reenter()->i32 contract{effects(ffi);};
extern "C" fn async_complete()->i32 contract{effects(ffi);};
extern "C" fn async_note(code:i32) contract{effects(ffi);};
extern "C" fn tuple_failure_complete()->i32 contract{effects(ffi);};
extern "C" fn putchar(code:i32)->i32;
extern "C" fn tuple_close_complete()->i32 contract{effects(ffi);};
resource Marker {code:i32}
impl Drop for Marker {fn drop(self:Self){unsafe{putchar(.code);}}}
resource Mark {code:i32}
impl Drop for Mark {fn drop(self:Self){unsafe{async_note(.code);}}}
extern "C" fn async_result_drop(code:i32) contract{effects(ffi);};
resource Answer {code:i32}
impl Drop for Answer {fn drop(self:Self){unsafe{async_result_drop(.code);}}}
enum Packet { Value(manages Answer) }
package Leaf { manages mark:Mark; adjustment:i32 }
package Bundle { leaf:Leaf; adjustment:i32 }
enum WorkerFailure { Stopped }
fn worker(seed:i32)->WorkerFailure!Packet {
    let mark=Mark{code:37};
    let bundle=Bundle{leaf:Leaf{mark:give mark,adjustment:1},adjustment:2};
    var count=seed;
    let callback=move(value:i32)->i32=>{count+=1; unsafe {if async_reenter()!=0 {return -100}} return value+count+bundle.leaf.mark.code-37};
    unsafe {
    var owner=c_register_async(async_register,async_request,async_drain,callback);
    if !async_registration_valid(owner) {return WorkerFailure.Stopped;}
    let marker=Marker{code:65};
    let pair=(give marker,give owner);
    let extracted=give pair.0;
    task_yield();
    return WorkerFailure.Stopped;
    }
}
extern "C" fn async_seen(value:i32) contract{effects(ffi);};
fn drive(task:&mut Task<WorkerFailure!Packet>)->TaskPollState!i32 {
    match task_poll(task)? {
        std.task.Poll.Pending()=>{unsafe{async_seen(0);}return 0;},
        std.task.Poll.Ready(result)=>{ match result { .ok(_)=>{return 71;}, .err(_)=>{unsafe{async_seen(17);}return 17;} } }
    }
}
fn main()->std.task.TaskCreateError!i32 {
    var task=task_create(worker,37)?;
    drive(&mut task);
    drive(&mut task);
    unsafe {if async_pump()!=0 {task_finish_destroy(give task);return 3}}
    drive(&mut task);
    match task_poll_close(&mut task) {
        std.task.Poll.Pending()=>{task_finish_destroy(give task);return 4;},
        std.task.Poll.Ready(_)=>{}
    }
    task_finish_destroy(give task);
    unsafe {return tuple_failure_complete()}
}
