fn worker<life a,life b>(prefix:i32,scratch:&'a i32,pointer:&'b i32)->&'b i32{
 task_yield()
 let observed=*scratch
 return pointer
}
fn attempt<life a,life b>(scratch:&'a mut i32,pointer:&'b mut i32)->i32{
 let created=task_create(worker,3,&*scratch,&*pointer)
 match created{
  .ok(task)=>{
   var owner=give task
   task_request_cancel(&mut owner)
   match task_poll_close(&mut owner){Poll.Pending()=>{return 1},Poll.Ready(_)=>{}}
   task_finish_destroy(give owner)
   *scratch=9
   *pointer=7
   return 0
  },
  .err(_)=>{
   *scratch=9
   *pointer=7
   return 0
  }
 }
}
fn main()->i32{
 var scratch=0
 var number=0
 let answer=attempt(&mut scratch,&mut number)
 return answer+number-7+scratch-9
}
