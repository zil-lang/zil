
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
fn fresh<T:StoreElement>(flag:bool)->Store<T> contract{result(create)}{if flag{let first:Store<T>=empty();let second=give first;return give second}return store_new()}fn main()->i32{let a:Store<Item>=fresh(true);reserve(a,1);push(a,item(65));let b:Store<Other>=fresh(false);reserve(b,1);push(b,other(66));return i32(len(a)+len(b))-2}