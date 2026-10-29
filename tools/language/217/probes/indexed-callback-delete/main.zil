extern "C" fn putchar(code:i32)->i32;
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
package Node<F>{manages mark:Mark;callback:F;id:i32;parent:i32}
fn counter(start:i32)->impl FnMut()->i32{var n=start;return move()=>{n+=1;return n}}
fn node<F:FnMut()->i32>(f:F,id:i32,parent:i32)->Node<F>{return Node::<F>{mark:Mark{code:64+id},callback:f,id:id,parent:parent}}
fn seed<T:StoreElement>(value:T)->Store<T> contract{result(create)}{let items:Store<T>=store_new();store_reserve(items,8);store_push(items,value);return give items}
fn release<T:StoreElement>(item:T){}
fn erase<T:StoreElement>(items:Store<T>,at:i64)->void!i32{
 let count=store_len(items)
 if at<0||at>=count{return 9}
 let suffix:Store<T>=store_new()
 if !store_reserve(suffix,count){return 1}
 while store_len(items)>at+1{let moved=store_pop(items)?;store_push(suffix,moved)}
 release(store_pop(items)?)
 while store_len(suffix)>0{let moved=store_pop(suffix)?;store_push(items,moved)}
 return 0
}
fn main()->void!i32{
 let items=seed(node(counter(10),1,0))
 store_push(items,node(counter(20),2,1))
 store_push(items,node(counter(30),3,1))
 if store_at_mut(items,0).callback()!=11{return 2}
 if store_at_mut(items,1).callback()!=21{return 3}
 if store_at_mut(items,2).callback()!=31{return 4}
 let removed=erase(items,1)?
 if removed!=0{return 5}
 if store_len(items)!=2{return 6}
 if store_at(items,0).id!=1||store_at(items,1).id!=3{return 7}
 if store_at(items,1).parent!=1{return 8}
 if store_at_mut(items,0).callback()!=12{return 10}
 if store_at_mut(items,1).callback()!=32{return 11}
 unsafe{putchar(33)}
 return 0
}
