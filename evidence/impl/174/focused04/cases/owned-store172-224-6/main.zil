
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R;n:i32}
fn make(code:i32)->P=>P{r:R{code:code},n:code}
fn empty()->Store<P> contract{result(create)}{return store_new()}
 fn main()->void!i32{let s=empty();if !store_reserve(s,1){return ()};store_push(s,make(65));if store_reserve(s,-1)||store_reserve(s,9223372036854775807){return ()};if store_len(s)!=1||store_capacity(s)!=1{return ()};let p=store_pop(s)?;return p.n-65}