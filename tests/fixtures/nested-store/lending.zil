extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket;value:i32}
package Row{manages inner:Store<Item>;manages mark:Ticket}
package Catalog{manages rows:Store<Row>}
fn peek<life a>(catalog:&'a Catalog,index:i64)->Option<&'a Item>{
 if index<0 || index>=store_len(catalog.rows){return Option.None()}
 let row=store_at(catalog.rows,index)
 if store_len(row.inner)==0{return Option.None()}
 let item=store_at(row.inner,0)
 return Option.Some(item)
}
fn forward<life a>(catalog:&'a Catalog,index:i64)->Option<&'a Item>{return peek(catalog,index)}
fn main()->i32{
 let inner:Store<Item>=store_new()
 if !store_reserve(inner,1){return 1}
 store_push(inner,Item{ticket:Ticket{code:65},value:42})
 let rows:Store<Row>=store_new()
 if !store_reserve(rows,1){return 2}
 store_push(rows,Row{inner:give inner,mark:Ticket{code:82}})
 let catalog=Catalog{rows:give rows}
 let saved=forward(&catalog,0)
 match saved{Option.Some(item)=>{return item.value-42},Option.None()=>{return 3}}
}
