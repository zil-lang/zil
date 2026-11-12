struct Holder<life b>{pointer:&'b i32}
fn hold<life b>(pointer:&'b i32)->Holder<'b>{return Holder::<'b>{pointer:pointer}}
fn make<life a,life b>(holder:&'a mut Holder<'b>)->impl FnOnce()->&'a mut Holder<'b>{
 return move()=>give holder
}
fn main()->i32{
 var value=20
 var other=22
 var holder=hold(&value)
 let f=make(&mut holder)
 let pointer=f()
 let data=pointer.pointer
 holder.pointer=&other
 let old=*data
 value=21
 return old+*holder.pointer+value-63
}
