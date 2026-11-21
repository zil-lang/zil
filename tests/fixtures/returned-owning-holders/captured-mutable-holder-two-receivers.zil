struct Holder<life a>{pointer:&'a i32}
fn hold<life a>(pointer:&'a i32)->Holder<'a>{return Holder::<'a>{pointer:pointer}}
fn make<life a>(holder:&mut Holder<'a>,other:&'a i32)->impl FnMut()->i32{
 return move()=>{let old=holder.pointer;holder.pointer=other;return *old}
}
fn invoke<F:FnMut()->i32>(f:&mut F)->i32{return f()} fn relay<F:FnMut()->i32>(f:&mut F)->i32{return invoke(f)}fn main()->i32{var a=20;var b=22;var c=30;var d=32;var holder=hold(&a);var another=hold(&c);var f=make(&mut holder,&b);var g=make(&mut another,&d);let first=relay(&mut f);a=21;let third=relay(&mut g);c=31;let second=relay(&mut f);let fourth=relay(&mut g);let x=*holder.pointer;let y=*another.pointer;b=23;d=33;return first+second+third+fourth+x+y+a+b+c+d-266}
