extern "C" fn reg(cb:extern "C" fn(c_handle,i32)->i64,ctx:c_handle)->c_handle contract{provides(static_callbacks(),async_callback_register());effects(ffi);};
extern "C" fn request(token:c_handle) contract{provides(static_callbacks(),async_callback_request_close());effects(ffi);};
extern "C" fn drain(token:c_handle)->i32 contract{provides(static_callbacks(),async_callback_poll_drain());effects(ffi);};
fn safe_worker()->i32 {task_yield();return 0;}
fn async_worker<F:FnMut(i32)->i32 + ClosedEnvironment>(give owner:AsyncRegistration<F>)->i32 contract{requires(state(owner,Open,Closing,Closed));provides(retired(owner));} {
 var remaining=give owner;task_yield();async_registration_request_close(&mut remaining);
 loop{match async_registration_poll_close(&mut remaining){Poll.Pending()=>{task_yield();continue;},Poll.Ready(_)=>{break;}}}
 async_registration_finish_destroy_checked(give remaining);return 0;
}
fn make_safe_child()->TaskCreateError!Managed<Task<i32>> {return task_create(safe_worker);}
fn make_async_child()->TaskCreateError!Managed<Task<i32>> {unsafe{
 let callback=move(value:i32)->i32=>value;
 let owner=c_register_async(reg,request,drain,callback);
 match task_create_owned(async_worker,give owner){
  .err(failure)=>{let reason=failure.reason;let (input,)=failure.inputs;var remaining=give input;
   async_registration_request_close(&mut remaining);
   loop{match async_registration_poll_close(&mut remaining){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
   async_registration_finish_destroy_checked(give remaining);return reason;
  },
  .ok(input)=>{return give input;}
 }
}}
fn close(give original:Task<i32>)->i32 {
 var task=give original;task_request_cancel(&mut task);
 loop{match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
 task_finish_destroy(give task);return 0;
}
extern "C" fn quiet_retired(code:i32);
extern "C" fn result_retired(code:i32);
extern "C" fn result_seen(code:i32);
resource Quiet {value:i32}
impl Drop for Quiet {fn drop(self:Self){unsafe{quiet_retired(.value);}}}
resource ResultToken {code:i32}
impl Drop for ResultToken {fn drop(self:Self){unsafe{result_retired(.code);}}}
package Packet {manages token:ResultToken}
fn keep(give original:Task<i32>,give selected:Task<i32>,give quiet:Quiet)->impl FnOnce()->Managed<Task<i32>> {
 var task=give original;var kept=give quiet;
 return move()->Managed<Task<i32>>=>{
  task_valid(&task);kept.value+=1;return give selected;
 };
}
fn parent<F:FnOnce()->Managed<Task<i32>> + ClosedEnvironment>(give callback:F)->i32 {
 task_yield();let answer=callback();close(give answer);unsafe{result_seen(17);}return 0;
}
fn run(give original:Task<i32>)->i32 {
 var task=give original;var answer=91;
 loop{match task_poll(&mut task){
  .err(_)=>{answer=92;break;},
  .ok(poll)=>{match poll{Poll.Pending()=>{continue;},Poll.Ready(value)=>{answer=value;break;}}}
 }}
 close(give task);return answer;
}
fn main()->TaskCreateError!i32 {
 let original=make_async_child()?;
 match make_safe_child(){
  .err(reason)=>{close(give original);return reason;},
  .ok(input)=>{
   var selected=give input;
   let quiet=Quiet{value:7};var callback=keep(give original,give selected,give quiet);
   loop{match task_create_owned(parent,give callback){
    .err(failure)=>{let (input,)=failure.inputs;callback=give input;continue;},
    .ok(input)=>{var task=give input;task_poll(&mut task);return run(give task);}
   }}
  }
 }
}
