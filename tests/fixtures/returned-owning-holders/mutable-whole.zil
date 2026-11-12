extern "C" fn putchar(value:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Mixed<life a>{manages owned:R;pointer:&'a i32}
fn wrap<life a>(pointer:&'a i32)->Mixed<'a>{return Mixed::<'a>{owned:R{code:65},pointer:pointer}}
fn make<life a>(input:Mixed<'a>,other:&'a i32)->impl FnMut()->i32{
 var holder=input
 return move()=>{let old=holder.pointer;holder.pointer=other;return *old}
}
fn main()->i32{let a=20;let b=22;var f=make(wrap(&a),&b);let first=f();let second=f();return first+second-42}
