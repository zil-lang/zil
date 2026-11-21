struct Holder<life a>{pointer:&'a mut i32}
fn hold<life a>(pointer:&'a mut i32)->Holder<'a>{return Holder::<'a>{pointer:pointer}}
fn make<life a>(holder:&mut Holder<'a>,other:&'a mut i32)->impl FnMut()->i32{
 return move()=>{holder.pointer=other;return 0}
}
fn main()->i32{
 var a=20;var b=22;var holder=hold(&mut a)
 var f=make(&mut holder,&mut b)
 f();return f()
}
