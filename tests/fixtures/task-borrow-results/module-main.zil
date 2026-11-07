import api
fn main()->TaskCreateError!i32{
 let number=42
 let unrelated=5
 let ordinary=&unrelated
 let check=*ordinary
 var task=api.relay(api.start(&number)?)
 match api.poll(&mut task){
  .ok(value)=>{match value{Poll.Pending()=>{},Poll.Ready(_)=>{return 91}}},
  .err(_)=>{return 92}
 }
 match api.poll(&mut task){
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
