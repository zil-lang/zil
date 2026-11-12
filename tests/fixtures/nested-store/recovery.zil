extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket;value:i32}
package Row{manages inner:Store<Item>;manages mark:Ticket}
fn ensure_items(items:Store<Item>,capacity:i64,expected:i32)->bool{
 let length=store_len(items)
 let previous=store_capacity(items)
 if store_reserve(items,capacity){return true}
 if store_len(items)!=length || store_capacity(items)!=previous{return false}
 if length>0{let item=store_at(items,0);if item.value!=expected{return false}}
 return store_reserve(items,capacity)
}
fn ensure_rows(rows:Store<Row>,capacity:i64)->bool{
 let length=store_len(rows)
 let previous=store_capacity(rows)
 if store_reserve(rows,capacity){return true}
 if store_len(rows)!=length || store_capacity(rows)!=previous{return false}
 if length>0{let row=store_at(rows,0);let item=store_at(row.inner,0);if item.value!=20{return false}}
 if length>1{let row=store_at(rows,1);let item=store_at(row.inner,0);if item.value!=22{return false}}
 return store_reserve(rows,capacity)
}
fn main()->i32{
 let first:Store<Item>=store_new()
 if !ensure_items(first,1,20){return 1}
 store_push(first,Item{ticket:Ticket{code:65},value:20})
 let second:Store<Item>=store_new()
 if !ensure_items(second,1,22){return 2}
 store_push(second,Item{ticket:Ticket{code:66},value:22})
 let rows:Store<Row>=store_new()
 if !ensure_rows(rows,1){return 3}
 store_push(rows,Row{inner:give first,mark:Ticket{code:82}})
 // The full prefix returns this complete Row and its actual child heap.
 match store_push(rows,Row{inner:give second,mark:Ticket{code:83}}){
  .err(row)=>{
   let item=store_at(row.inner,0)
   if item.value!=22{return 4}
   if !ensure_rows(rows,2){return 5}
   match store_push(rows,row){.ok(_)=>{},.err(_)=>{return 6}}
  },
  .ok(_)=>{return 7}
 }
 if true{let row=store_at_mut(rows,0);if !ensure_items(row.inner,4,20){return 8}}
 if !ensure_rows(rows,4){return 9}
 var total:i32=0
 match store_pop(rows){
  .ok(row)=>{
   let inner=give row.inner
   match store_pop(inner){.ok(item)=>{total+=item.value},.err(_)=>{return 10}}
  },
  .err(_)=>{return 11}
 }
 match store_pop(rows){.ok(row)=>{let item=store_at(row.inner,0);total+=item.value},.err(_)=>{return 12}}
 return total-42
}
