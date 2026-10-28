
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages r:R;n:i32}
package Other{manages r:R;wide:i64}
fn item(n:i32)->Item=>Item{r:R{code:n},n:n}
fn other(n:i32)->Other=>Other{r:R{code:n},wide:i64(n)}
fn bad<T:StoreElement>(give s:Store<T>)->Store<T>contract{result(give)}{let a=give s;return give s};fn main(){}