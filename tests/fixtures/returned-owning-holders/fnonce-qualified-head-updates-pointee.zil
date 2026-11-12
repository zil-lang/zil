struct Holder<life b>{pointer:&'b i32}
fn hold<life b>(pointer:&'b i32)->Holder<'b>{return Holder::<'b>{pointer:pointer}}
fn make<life a,life b>(holder:&'a mut Holder<'b>,other:&'b i32)->impl FnOnce()->&'a mut Holder<'b>{
 return move()=>{
  let pointer=give holder
  pointer.pointer=other
  return give pointer
 }
}
fn main()->i32{
 var value=20
 let other=22
 var holder=hold(&value)
 let f=make(&mut holder,&other)
 let pointer=f()
 if *pointer.pointer!=22{return 1}
 value=21
 return value+*pointer.pointer-43
}
