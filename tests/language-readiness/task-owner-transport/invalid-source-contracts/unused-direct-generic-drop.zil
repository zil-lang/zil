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
fn parent<F:FnOnce()->i32 + ClosedEnvironment>(callback:F)->i32 {return 0;}
fn close_callback<F:FnOnce()->i32 + ClosedEnvironment>(callback:F)->i32 {return callback();}
fn main()->i32 {return 0;}
