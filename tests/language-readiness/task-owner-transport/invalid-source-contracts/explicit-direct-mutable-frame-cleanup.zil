extern "C" fn putchar(code:i32)->i32;
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code);}}}
fn worker(input:&mut i32,give token:Token)->i32 {*input+=5;task_yield();return *input+token.code-65;}
fn keep<life a>(give original:Task<i32,&'a mut i32>)->impl FnMut()->i32 {
 var task=give original;
 return move()->i32=>{
  match task_poll(&mut task){
   .ok(p)=>{match p{Poll.Pending()=>{return -1;},Poll.Ready(value)=>{return value;}}},
   .err(_)=>{return -2;}
  }
 };
}
fn drive_callback<life a>(give original:Task<i32,&'a mut i32>)->i32 contract{effects(io);}{
 var callback=keep(give original);return 0;
}
fn main()->TaskCreateError!i32 {
 var number=37;let input=Token{code:65};var original=task_create(worker,&mut number,give input)?;let answer=drive_callback(give original);
 if number!=37{return 92;}number=9;return answer;
}
