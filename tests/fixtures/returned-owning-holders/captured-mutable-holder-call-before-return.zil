struct Holder<life a>{pointer:&'a i32}
fn hold<life a>(pointer:&'a i32)->Holder<'a>{return Holder::<'a>{pointer:pointer}}
fn make<life a>(holder:&mut Holder<'a>,other:&'a i32)->impl FnMut()->i32{
 return move()=>{let old=holder.pointer;holder.pointer=other;return *old}
}
fn step<F:FnMut()->i32>(input:F)->F{var current=input;current();return current} fn relay<F:FnMut()->i32>(input:F)->F{return step(input)}fn main()->i32{var a=20;var b=22;var holder=hold(&a);var f=relay(make(&mut holder,&b));a=21;let n=f();let p=*holder.pointer;b=23;return n+p+a+b-88}
