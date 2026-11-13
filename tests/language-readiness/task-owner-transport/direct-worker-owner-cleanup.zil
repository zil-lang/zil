extern "C" fn quiet_retired(code:i32);
resource Quiet {value:i32}
impl Drop for Quiet {fn drop(self:Self){unsafe{quiet_retired(.value);}}}
fn child()->i32 {task_yield();return 7;}
fn close(give original:Task<i32>)->i32 {
 var task=give original;task_request_cancel(&mut task);
 loop{match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
 task_finish_destroy(give task);return 0;
}
fn keep(give original:Task<i32>,give quiet:Quiet)->impl FnOnce()->i32 {
 var task=give original;var kept=give quiet;
 return move()->i32=>{
  task_request_cancel(&mut task);
  loop{match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
  kept.value+=1;return kept.value;
 };
}
fn parent<F:FnOnce()->i32 + ClosedEnvironment>(give callback:F)->i32 {return 0;}
fn close_callback<F:FnOnce()->i32 + ClosedEnvironment>(give callback:F)->i32 {return callback();}
fn main()->TaskCreateError!i32 {
 let quiet=Quiet{value:7};let original=task_create(child)?;
 let callback=keep(give original,give quiet);
 match task_create_owned(parent,give callback){
  .err(failure)=>{let reason=failure.reason;let (input,)=failure.inputs;close_callback(give input);return reason;},
  .ok(input)=>{var task=give input;task_poll(&mut task);return close(give task);}
 }
}
