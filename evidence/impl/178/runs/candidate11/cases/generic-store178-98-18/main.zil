
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages r:R;n:i32}
package Other{manages r:R;wide:i64}
fn item(n:i32)->Item=>Item{r:R{code:n},n:n}
fn other(n:i32)->Other=>Other{r:R{code:n},wide:i64(n)}

fn empty<T:StoreElement>()->Store<T> contract{result(create)}=>store_new()
fn reserve<T:StoreElement>(s:Store<T>,n:i64)->bool=>store_reserve(s,n)
fn push<T:StoreElement>(s:Store<T>,v:T)->T!void=>store_push(s,v)
fn pop<T:StoreElement>(s:Store<T>)->void!T=>store_pop(s)
fn len<T:StoreElement>(s:Store<T>)->i64=>store_len(s)
fn relay<T:StoreElement>(give s:Store<T>)->Store<T> contract{result(give)}=>give s

 package Batch<T:StoreElement>{manages items:Store<T>}
 fn batch<T:StoreElement>(v:T)->Batch<T>{let s:Store<T>=empty();reserve(s,1);push(s,v);return Batch::<T>{items:give s}}
 fn take<T:StoreElement>(b:Batch<T>)->Store<T>contract{result(give)}{return give b.items}
 package State<F>{manages items:Store<Item>;callback:F;n:i32}
 fn make()->impl FnMut()->i32{var n=0;return move()=>{n+=1;return n}}
 fn pack<F:FnMut()->i32>(f:F)->State<F>{let s:Store<Item>=empty();reserve(s,1);push(s,item(65));return State::<F>{items:give s,callback:f,n:0}}
 fn update<F:FnMut()->i32>(state:State<F>)->State<F>{var p=state;let old=give p.items;p.items=empty();reserve(p.items,1);push(p.items,item(66));let f=&mut p.callback;p.n=f();return p}
 fn main()->i32{let a=take(batch(other(67)));let p=update(pack(make()));return p.n+i32(len(a))+i32(store_len(p.items))-3}
 