fn worker<life a>(pointer:&'a i32)->&'a i32{
 let local=7
 task_yield()
 return pointer
}
fn start<life a>(pointer:&'a i32)->TaskCreateError!Task<&'a i32,&'a i32> contract{result(give)}{
 return task_create(worker,pointer)
}
fn relay<life a>(give task:Task<&'a i32,&'a i32>)->Task<&'a i32,&'a i32> contract{result(give)}{
 return give task
}
fn poll<life a>(task:&mut Task<&'a i32,&'a i32>)->TaskPollState!Poll<&'a i32>{
 return task_poll(task)
}
fn main()->TaskCreateError!i32{
 let number=42
 let unrelated=5
 let ordinary=&unrelated
 let check=*ordinary
 var task=relay(start(&number)?)
 match poll(&mut task){
  .ok(value)=>{match value{Poll.Pending()=>{},Poll.Ready(_)=>{return 91}}},
  .err(_)=>{return 92}
 }
 match poll(&mut task){
  .ok(value)=>{match value{
   Poll.Pending()=>{return 93},
   Poll.Ready(pointer)=>{
    match task_poll_close(&mut task){Poll.Pending()=>{return 94},Poll.Ready(_)=>{}}
    task_finish_destroy(give task)
    return *pointer-42+check-5
   }
  }},
  .err(_)=>{return 95}
 }
}
