fn worker<life a>(pointer:&'a i32)->&'a i32{
 task_yield()
 return pointer
}
fn attempt<life a>(pointer:&'a mut i32)->i32{
 let created=task_create(worker,&*pointer)
 match created{
  .ok(task)=>{
   var owner=give task
   task_request_cancel(&mut owner)
   match task_poll_close(&mut owner){Poll.Pending()=>{return 1},Poll.Ready(_)=>{}}
   task_finish_destroy(give owner)
   *pointer=7
   return 0
  },
  .err(_)=>{
   *pointer=7
   return 0
  }
 }
}
fn main()->i32{
 var number=0
 let answer=attempt(&mut number)
 return answer+number-7
}
