import std.iter
extern "C" fn putchar(value:i32)->i32;
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages mark:Mark;value:i32}
package Generate{manages mark:Mark;sent:bool}
impl iter.Iterator for Generate{
 type Item:Value+SyncDrop=iter.Drain<Item>;
 fn next(self:&mut Self)->void!iter.Drain<Item>{
  if self.sent{return ()}
  self.sent=true
  let values:Store<Item>=store_new()
  if !store_reserve(values,1){return ()}
  store_push(values,Item{mark:Mark{code:65},value:42})
  return iter.drain(give values)
 }
}
package Keep{manages mark:Mark}
impl iter.Mapper<iter.Drain<Item>> for Keep{
 type Output:Value+SyncDrop=iter.Drain<Item>;
 fn map(self:&mut Self,value:iter.Drain<Item>)->iter.Drain<Item>{return value}
}
fn main()->i32{
 var cursor=iter.map(Generate{mark:Mark{code:71},sent:false},Keep{mark:Mark{code:75}})
 var total:i32=0
 var take=(value:iter.Drain<Item>)=>{
  var current=value
  match current.next(){.ok(item)=>{total+=item.value},.err(_)=>{}}
 }
 iter.for_each_owned(&mut cursor,&mut take)
 return total-42
}
