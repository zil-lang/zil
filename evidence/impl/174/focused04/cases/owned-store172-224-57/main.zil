
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R;n:i32}
fn make(code:i32)->P=>P{r:R{code:code},n:code}
fn empty()->Store<P> contract{result(create)}{return store_new()}
 fn relay(give s:Store<P>)->Store<P> contract{result(give)}{return give s};fn main()->i32{let s=empty();if !store_reserve(s,2){return 7};store_push(s,make(65));store_push(s,make(66));let moved=relay(give s);return i32(store_len(moved))-2}