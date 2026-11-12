fn make()->impl Fn()->u8{let text="abc";return move()=>text[0];}
fn worker<F:Fn()->u8 + ClosedEnvironment>(callback:F)->i32 {
 task_yield();return i32(callback());
}
fn main()->TaskCreateError!i32 {
 var task=task_create(worker,make())?;var answer=0;
 loop{match task_poll(&mut task){.ok(p)=>{match p{Poll.Pending()=>{continue;},Poll.Ready(v)=>{answer=v;break;}}},.err(_)=>{return 91;}}}
 task_request_cancel(&mut task);
 loop{match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
 task_finish_destroy(give task);return answer-97;
}
