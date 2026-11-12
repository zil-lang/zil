extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket;value:i32}
package Catalog{manages rows:Store<Item>}
fn make(value:&i32)->impl Fn()->i32{return move()=>*value}
fn grow(rows:Store<Item>)->bool{return store_reserve(rows,2)}
fn peek<life a>(catalog:&'a Catalog)->Option<&'a Item>{
 if store_len(catalog.rows)==0{return Option.None()}
 return Option.Some(store_at(catalog.rows,0))
}
fn main()->i32{
 let expected=42
 let read=make(&expected)
 let rows:Store<Item>=store_new()
 if !grow(rows){return 1}
 store_push(rows,Item{ticket:Ticket{code:65},value:42})
 let catalog=Catalog{rows:give rows}
 let saved=peek(&catalog)
 match saved{Option.Some(item)=>{return item.value-read()},Option.None()=>{return 2}}
}
