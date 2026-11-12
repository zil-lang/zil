fn worker(input:&mut i32)->i32 {*input+=5;task_yield();return *input;}
fn keep<life a>(give original:Task<i32,&'a mut i32>)->impl FnMut()->i32 {
 var task=give original;
 return move()->i32=>{
  match task_poll(&mut task){
   .ok(p)=>{match p{Poll.Pending()=>{return -1;},Poll.Ready(value)=>{return value;}}},
   .err(_)=>{return -2;}
  }
 };
}
fn use<life a>(give original:Task<i32,&'a mut i32>)->i32 {
 var callback=keep(give original);if callback()!=-1{return 91;}return callback();
}
fn main()->TaskCreateError!i32 {
 var number=37;let original=task_create(worker,&mut number)?;let answer=use(give original);
 if number!=42{return 92;}number=9;return answer-42;
}
