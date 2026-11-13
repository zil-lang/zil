fn worker()->i32 {task_yield();return 42;}
fn drive(give original:Task<i32>)->i32 {
 var task=give original;var answer=0;
 loop {match task_poll(&mut task){.ok(p)=>{match p{Poll.Pending()=>{continue;},Poll.Ready(value)=>{answer=value;break;}}},.err(_)=>{break;}}}
 task_request_cancel(&mut task);
 loop {match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
 task_finish_destroy(give task);return answer;
}
fn keep(give original:Task<i32>)->impl FnOnce()->i32 {
 return move()=>drive(give original);
}
fn main()->TaskCreateError!i32 {
 let original=task_create(worker)?;
 let callback=keep(give original);let answer=callback();
 return answer-42;
}
