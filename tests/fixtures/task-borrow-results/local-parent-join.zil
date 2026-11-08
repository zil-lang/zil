fn worker<life a>(pointer:&'a i32)->&'a i32{
 task_yield()
 return pointer
}
fn attempt<life a>(first:&'a mut i32,second:&'a mut i32,spare:&'a mut i32,flag:bool)->i32{
 var input=&*first
 if flag{input=&*second}
 *spare=*spare+1
 let selected=*input
 let created=task_create(worker,input)
 match created{
  .ok(task)=>{
   var owner=give task
   task_request_cancel(&mut owner)
   match task_poll_close(&mut owner){Poll.Pending()=>{return 91},Poll.Ready(_)=>{}}
   task_finish_destroy(give owner)
  },
  .err(_)=>{}
 }
 let observed=*input
 *first=*first+1
 *second=*second+1
 return selected+observed
}
fn main()->i32{
 var left=4
 var right=9
 var spare=0
 let first=attempt(&mut left,&mut right,&mut spare,true)
 let second=attempt(&mut left,&mut right,&mut spare,false)
 return first+second+left+right+spare-47
}
