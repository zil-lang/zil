extern "C" fn putchar(value:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Mixed<life a>{manages owned:R;pointer:&'a i32}
fn wrap<life a>(pointer:&'a i32)->Mixed<'a>{return Mixed::<'a>{owned:R{code:65},pointer:pointer}}
fn make<life a>(input:Mixed<'a>,other:&'a i32)->impl FnMut()->i32{
 var holder=input
 return move()=>{let old=holder.pointer;holder.pointer=other;return *old}
}
fn step<F:FnMut()->i32>(input:F)->F{var current=input;current();return current}
fn through<F:FnMut()->i32>(input:F)->F{return step(input)}
fn main()->i32{var a=20;var b=22;var f=through(make(wrap(&a),&b));a=21;let answer=f();b=23;return answer+a+b-66}
