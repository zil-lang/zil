
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages r:R;n:i32}
package Other{manages r:R;wide:i64}
fn item(n:i32)->Item=>Item{r:R{code:n},n:n}
fn other(n:i32)->Other=>Other{r:R{code:n},wide:i64(n)}
fn bad<T:StoreElement>(s:Store<T>,v:T)->void!void{store_push(s,v)?;return ()};fn main(){}