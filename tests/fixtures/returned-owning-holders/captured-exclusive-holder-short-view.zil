struct Holder<life a>{pointer:&'a mut i32}
fn hold<life a>(pointer:&'a mut i32)->Holder<'a>{return Holder::<'a>{pointer:pointer}}
fn make<life a>(holder:&mut Holder<'a>)->impl FnMut()->&i32{
 var calls=0
 return move()=>{calls+=1;return &*holder.pointer}
}
fn main()->i32{
 var value=21
 var holder=hold(&mut value)
 var f=make(&mut holder)
 let first=*f()
 let pointer=f()
 let second=*pointer
 let third=*f()
 return first+second+third-63
}
