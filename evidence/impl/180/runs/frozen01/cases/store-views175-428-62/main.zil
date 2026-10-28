
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R;n:i32}
fn make(code:i32)->P=>P{r:R{code:code},n:code}
fn empty()->Store<P> contract{result(create)}{return store_new()}
fn read(p:&P)->i32=>p.n
fn use_pair(view:&P,s:Store<P>)->i32{store_reserve(s,4);return view.n}fn main()->i32{let s=empty();store_reserve(s,2);store_push(s,make(65));store_push(s,make(66));let view=store_at(s,0);return use_pair(view,s)}