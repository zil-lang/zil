fn worker(input:&mut i32)->i32 {*input+=5;task_yield();return *input;}
fn drive<life a>(give original:Task<i32,&'a mut i32>)->i32 {
 var task=give original;var answer=0;
 loop {match task_poll(&mut task){.ok(p)=>{match p{Poll.Pending()=>{continue;},Poll.Ready(value)=>{answer=value;break;}}},.err(_)=>{break;}}}
 task_request_cancel(&mut task);
 loop {match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
 task_finish_destroy(give task);return answer;
}
fn keep<life a,life b>(give original:Task<i32,&'a mut i32>,give second:Task<i32,&'b mut i32>)->impl FnOnce()->Managed<Task<i32,&'b mut i32>> {
 return move()=>{drive(give original);return give second;};
}
fn main()->TaskCreateError!i32 {
 var number=37;var other=41;let original=task_create(worker,&mut number)?;
 let second=task_create(worker,&mut other)?;let callback=keep(give original,give second);let restored=callback();if number!=42{return 90;}number=9;let answer=drive(give restored);
 if other!=46{return 92;}return answer-46;
}
