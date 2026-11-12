extern "C" fn putchar(value:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Mixed<life a>{manages owned:R;pointer:&'a i32}
fn wrap<life a>(pointer:&'a i32)->Mixed<'a>{return Mixed::<'a>{owned:R{code:65},pointer:pointer}}
fn make<life a>(input:Mixed<'a>,other:&'a i32)->impl FnMut()->i32{
 var holder=input
 return move()=>{let old=holder.pointer;holder.pointer=other;return *old}
}
fn nested<F:FnMut()->i32>(input:F)->impl FnMut()->i32{var current=input;return move()=>current()}
fn main()->i32{
 var a=20;var b=22;var f=nested(make(wrap(&a),&b))
 let first=f();a=21;let second=f();b=23
 return first+second+a+b-86
}
