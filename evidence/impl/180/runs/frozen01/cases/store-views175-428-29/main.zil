
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R;n:i32}
fn make(code:i32)->P=>P{r:R{code:code},n:code}
fn empty()->Store<P> contract{result(create)}{return store_new()}
fn read(p:&P)->i32=>p.n
fn change(x:Store<P>){store_reserve(x,4)}fn use_pair(a:Store<P>,b:Store<P>)->i32{let view=store_at(a,0);change(b);return read(view)}fn main()->i32{let s=empty();store_reserve(s,2);store_push(s,make(65));store_push(s,make(66));return use_pair(s,s)}