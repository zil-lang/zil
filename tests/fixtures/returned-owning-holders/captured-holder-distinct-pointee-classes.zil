struct Holder<life a>{pointer:&'a i32}
fn hold<life a>(pointer:&'a i32)->Holder<'a>{return Holder::<'a>{pointer:pointer}}
fn make<life a,life b>(holder:&mut Holder<'a>,other:&'b i32)->impl FnMut()->i32{
 return move()=>{let old=holder.pointer;holder.pointer=other;return *old}
}
fn main()->i32{
 var a=20;var b=22;var holder=hold(&a)
 var f=make(&mut holder,&b)
 let first=f();a=21;let second=f()
 let current=*holder.pointer;b=23
 return first+second+current+a+b-108
}
