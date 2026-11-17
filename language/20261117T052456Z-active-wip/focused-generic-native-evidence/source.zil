fn retain<T:Value+ClosedEnvironment>(give selected:Task<T>)->impl FnOnce()->Managed<Task<T>>{return move()=>give selected;}fn worker()->i32{task_yield();return 42;}
fn drive(give original:Task<i32>)->i32 {
 var task=give original;var answer=0;
 loop {match task_poll(&mut task){.ok(p)=>{match p{Poll.Pending()=>{continue;},Poll.Ready(value)=>{answer=value;break;}}},.err(_)=>{break;}}}
 task_request_cancel(&mut task);
 loop {match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
 task_finish_destroy(give task);return answer;
}
fn main()->TaskCreateError!i32{let original=task_create(worker)?;let callback=retain::<i32>(give original);let answer=callback();return drive(give answer)-42;}
