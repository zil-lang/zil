fn worker<life a>(input:&'a mut i32)->i32 {*input+=5;task_yield();return *input;}
fn drive<life a>(give original:Task<i32,&'a mut i32>)->i32 {
 var task=give original;var answer=0;
 loop {match task_poll(&mut task){.ok(p)=>{match p{Poll.Pending()=>{continue;},Poll.Ready(value)=>{answer=value;break;}}},.err(_)=>{break;}}}
 task_request_cancel(&mut task);
 loop {match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
 task_finish_destroy(give task);return answer;
}
fn keep<life a>(give original:Task<i32,&'a mut i32>,give second:Task<i32,&'a mut i32>)->impl FnOnce()->i32 {
 return move()=>drive(give original)+drive(give second);
}
fn pair<life a>(first:&'a mut i32,second:&'a mut i32)->TaskCreateError!i32 {
 let original=task_create(worker::<'a>,first)?;
 let otherTask=task_create(worker::<'a>,second)?;
 let callback=keep(give original,give otherTask);return callback();
}
fn main()->TaskCreateError!i32 {
 var number=37;var other=41;let answer=pair(&mut number,&mut other)?;
 if number!=42{return 91;}number=9;if other!=46{return 92;}return answer-88;
}
