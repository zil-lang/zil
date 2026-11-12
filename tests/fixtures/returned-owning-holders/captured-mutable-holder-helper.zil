struct Holder<life a>{pointer:&'a i32}
fn hold<life a>(pointer:&'a i32)->Holder<'a>{return Holder::<'a>{pointer:pointer}}
fn make<life a>(holder:&mut Holder<'a>,other:&'a i32)->impl FnMut()->i32{
 return move()=>{let old=holder.pointer;holder.pointer=other;return *old}
}
fn invoke<F:FnMut()->i32>(f:&mut F)->i32{return f()} fn relay<F:FnMut()->i32>(f:&mut F)->i32{return invoke(f)} fn main()->i32{
 var a=20;var b=22;var holder=hold(&a)
 var f=make(&mut holder,&b)
 let first=relay(&mut f);a=21;let second=relay(&mut f)
 let current=*holder.pointer;b=23
 return first+second+current+a+b-108
}
