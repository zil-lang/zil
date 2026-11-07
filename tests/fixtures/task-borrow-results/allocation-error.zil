fn worker<life a>(pointer:&'a i32)->&'a i32{
 task_yield()
 return pointer
}
fn attempt<life a>(parent:&'a mut i32)->i32{
 let created=task_create(worker,&*parent)
 match created{
  .ok(task)=>{
   var owner=give task
   task_request_cancel(&mut owner)
   match task_poll_close(&mut owner){Poll.Pending()=>{return 90},Poll.Ready(_)=>{}}
   task_finish_destroy(give owner)
   return 91
  },
  .err(_)=>{
   let before=*parent
   *parent=before+1
   let renewed=&*parent
   return *renewed-before-1
  }
 }
}
fn main()->i32{
 var number=42
 let answer=attempt(&mut number)
 return answer+number-43
}
