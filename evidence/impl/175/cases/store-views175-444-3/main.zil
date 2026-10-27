
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R;n:i32}
fn make(code:i32)->P=>P{r:R{code:code},n:code}
fn empty()->Store<P> contract{result(create)}{return store_new()}
fn read(p:&P)->i32=>p.n

package State{manages tag:R;manages left:Store<P>;manages right:Store<P>}
fn state()->State{
    let left=empty();store_reserve(left,1);store_push(left,make(65));
    let right=empty();store_reserve(right,1);store_push(right,make(66));
    return State{tag:R{code:90},left:give left,right:give right}
}
fn relay(value:State)->State=>value
fn consume(value:State){}fn main()->i32{var s=state();let view=store_at(s.left,0);store_pop(s.left);return read(view)}