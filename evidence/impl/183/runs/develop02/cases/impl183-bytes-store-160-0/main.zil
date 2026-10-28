
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
enum E{Fail}

package P{manages r:R}
fn bytes()->E!ByteBuffer contract{result(create)}{let b=bytes_new();if !bytes_reserve(b,2){return E.Fail};bytes_push(b,u8(65));return give b}
fn store()->E!Store<P> contract{result(create)}{return store_new()}
fn main()->E!i32{let b=bytes()?;let s=store()?;if !store_reserve(s,1){return E.Fail};store_push(s,P{r:R{code:66}});let w=bytes_view(b);unsafe{putchar(i32(w[0]))};return i32(bytes_len(b)+store_len(s))-2}
