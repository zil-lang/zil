struct Holder<life a>{pointer:&'a mut i32}
fn hold<life a>(pointer:&'a mut i32)->Holder<'a>{return Holder::<'a>{pointer:pointer}}
fn make<life a>(holder:&mut Holder<'a>,other:&'a mut i32)->impl FnOnce()->i32{
 return move()=>{holder.pointer=give other;return 0}
}
fn main()->i32{
 var a=20
 var b=22
 var holder=hold(&mut a)
 let f=make(&mut holder,&mut b)
 f()
 a=21
 let result=*holder.pointer
 return result+a-43
}
