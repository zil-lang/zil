import std.owning_map
import std.owning_entries
extern "C" fn putchar(value:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Key{manages ticket:Ticket;id:i64}
package Value{manages ticket:Ticket;number:i64}
impl owning_map.KeyEq for Key{fn same(self:&Self,other:&Self)->bool{return self.id==other.id}}
fn inspect<life s>(found:Option<&'s Value>)->i64{
 match found{Option.Some(value)=>{return value.number},Option.None()=>{return -1}}
}
fn worker<life worker_bound>(pointer:&'worker_bound i32)->&'worker_bound i32{
 task_yield()
 return pointer
}
fn attempt<life a>(parent:&'a mut i32)->i32{
 let spare=43
 let input=if *parent==42 {&*parent}else{&spare}
 let created=task_create(worker,input)
 match created{
  .ok(task)=>{
   var owner=give task
   match task_poll(&mut owner){
    .ok(value)=>{match value{Poll.Pending()=>{},Poll.Ready(_)=>{return 81}}},
    .err(_)=>{return 82}
   }
   var map=owning_map.new::<Key,Value>()
   let first=owning_entries.Entry::<Key,Value>{key:Key{ticket:Ticket{code:65},id:7},value:Value{ticket:Ticket{code:66},number:42}}
   match owning_map.insert_or_replace::<Key,Value>(&mut map,first){.ok(_)=>{},.err(_)=>{return 11}}
   let query=Key{ticket:Ticket{code:67},id:7}
   let found=owning_map.get::<Key,Value>(&map,&query)
   let observed=inspect(found)
   if observed!=42{return 12}
   let second=owning_entries.Entry::<Key,Value>{key:Key{ticket:Ticket{code:68},id:8},value:Value{ticket:Ticket{code:69},number:9}}
   match owning_map.insert_or_replace::<Key,Value>(&mut map,second){.ok(_)=>{},.err(_)=>{return 13}}
   let moved=map
   if owning_map.len::<Key,Value>(&moved)!=2{return 14}
   match task_poll(&mut owner){
    .ok(value)=>{match value{
     Poll.Pending()=>{return 83},
     Poll.Ready(pointer)=>{
      match task_poll_close(&mut owner){Poll.Pending()=>{return 84},Poll.Ready(_)=>{}}
      task_finish_destroy(give owner)
      let before=*input
      let observed=*pointer
      *parent=before+1
      return observed-before
     }
    }},
    .err(_)=>{return 85}
   }
  },
  .err(_)=>{
   let before=*input
   *parent=before+1
   return 0
  }
 }
}
fn main()->i32{
 var number=42
 let answer=attempt(&mut number)
 if answer!=0{return answer}
 return number-43
}
