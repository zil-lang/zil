import api
fn main()->TaskCreateError!i32{
 var scratch=9
 let scratch_parent=&mut scratch
 var number=42
 let p=&mut number
 let q=&mut *p
 let unrelated=5
 let ordinary=&unrelated
 let check=*ordinary
 var task=api.relay(api.start(&*scratch_parent,&*q)?)
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
    *scratch_parent=10
    let released=*scratch_parent
    let observed=*pointer
    *q=43
    let changed=*q
    let restored=*p
    return observed+changed+restored-128+check-5+released-10
   }
  }},
  .err(_)=>{return 95}
 }
}
