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
fn unborrowed_worker()->i32{
 task_yield()
 return 7
}
fn scalar_worker(a:&i32,b:&mut i32)->i32{
 task_yield()
 *b=*a+2
 return *b
}
fn exclusive_worker(input:&mut i32)->i32{
 task_yield()
 *input=*input+1
 return *input
}
fn scalar_relay<life a,life b>(give task:Task<i32,(&'a i32,&'b mut i32)>)->Task<i32,(&'a i32,&'b mut i32)> contract{result(give)}{
 return give task
}
fn main()->TaskCreateError!i32{
 let number=42
 var unrelated=5
 let ordinary=&unrelated
 let check=*ordinary
 let scalar_input=40
 var scalar_output=0
 var task=relay(start(&number)?)
 var scalar=scalar_relay(task_create(scalar_worker,&scalar_input,&mut scalar_output)?)
 let safe_read=scalar_input
 unrelated=13
 var unborrowed=task_create(unborrowed_worker)?
 match task_poll(&mut unborrowed){
  .ok(value)=>{match value{Poll.Pending()=>{},Poll.Ready(_)=>{return 71}}},
  .err(_)=>{return 72}
 }
 match task_poll(&mut unborrowed){
  .ok(value)=>{match value{Poll.Pending()=>{return 73},Poll.Ready(value)=>{if value!=7{return 74}}}},
  .err(_)=>{return 75}
 }
 match task_poll_close(&mut unborrowed){Poll.Pending()=>{return 76},Poll.Ready(_)=>{}}
 task_finish_destroy(give unborrowed)
 var observed=0
 match task_poll(&mut scalar){
  .ok(value)=>{match value{Poll.Pending()=>{},Poll.Ready(_)=>{return 81}}},
  .err(_)=>{return 82}
 }
 match task_poll(&mut scalar){
  .ok(value)=>{match value{Poll.Pending()=>{return 83},Poll.Ready(value)=>{observed=value}}},
  .err(_)=>{return 84}
 }
 match task_poll_close(&mut scalar){Poll.Pending()=>{return 85},Poll.Ready(_)=>{}}
 task_finish_destroy(give scalar)
 var canceled=task_create(exclusive_worker,&mut scalar_output)?
 task_request_cancel(&mut canceled)
 match task_poll_close(&mut canceled){Poll.Pending()=>{return 86},Poll.Ready(_)=>{}}
 task_finish_destroy(give canceled)
 scalar_output=43
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
    return *pointer-42+check-5+observed-42+scalar_output-43+safe_read-40
   }
  }},
  .err(_)=>{return 95}
 }
}
