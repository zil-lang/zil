import api
fn attempt<life a>(parent:&'a mut api.Value)->i32{
 let input=&*parent
 let created=task_create(api.worker,input)
 match created{
  .ok(task)=>{
   var owner=give task
   match task_poll(&mut owner){
    .ok(value)=>{match value{
     Poll.Pending()=>{return 83},
     Poll.Ready(pointer)=>{
      match task_poll_close(&mut owner){Poll.Pending()=>{return 84},Poll.Ready(_)=>{}}
      task_finish_destroy(give owner)
      let before=input.number
      let observed=pointer.number
      parent.number=before+1
      return observed-before
     }
    }},
    .err(_)=>{return 85}
   }
  },
  .err(_)=>{
   let before=input.number
   parent.number=before+1
   let renewed=&*parent
   return renewed.number-before-1
  }
 }
}
fn main()->i32{
 var number=api.Value{number:42}
 let answer=attempt(&mut number)
 return answer+number.number-43
}
