
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

 enum Kind{Empty,Value(i32)}struct Payload{pair:(i32,bool);bytes:[u8;2]}
 package ForeignItem{manages r:R;payload:Payload;kind:Kind;handle:c_handle;text:c_wstr;callback:extern "C" fn(i32)->i32}
 fn callback(n:i32)->i32=>n
 fn main()->i32{let s:Store<ForeignItem>=empty();reserve(s,1);push(s,ForeignItem{r:R{code:65},payload:Payload{pair:(7,true),bytes:[1u8,2]},kind:Kind.Value(3),handle:c_null(),text:c_wstr("ZIL"),callback:c_callback(callback)});return i32(len(s))-1}
 