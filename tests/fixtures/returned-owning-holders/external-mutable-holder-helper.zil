extern "C" fn putchar(value:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Mixed<life a>{manages owned:R;pointer:&'a i32}
fn wrap<life a>(pointer:&'a i32)->Mixed<'a>{return Mixed::<'a>{owned:R{code:65},pointer:pointer}}
fn rewrite<life a>(holder:&mut Mixed<'a>,other:&'a i32){holder.pointer=other}
fn make<life a>(input:Mixed<'a>,other:&'a i32)->impl FnMut()->i32{
 var holder=input
 return move()=>{let old=holder.pointer;rewrite(&mut holder,other);return *old}
}
fn invoke<F:FnMut()->i32>(input:&mut F)->i32{return input()}
fn relay<F:FnMut()->i32>(input:&mut F)->i32{return invoke(input)}
fn main()->i32{
 var a=20;var b=22;var c=30;var d=32
 var f=make(wrap(&a),&b);var g=make(wrap(&c),&d)
 let first=relay(&mut f);a=21
 let other=relay(&mut g);c=31
 let answer=relay(&mut f)+relay(&mut g)
 b=23;d=33
 return first+other+answer+a+c+b+d-212
}
