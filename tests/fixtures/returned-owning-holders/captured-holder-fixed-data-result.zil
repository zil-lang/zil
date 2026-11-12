struct Holder<life a>{pointer:&'a i32}
fn hold<life a>(pointer:&'a i32)->Holder<'a>{return Holder::<'a>{pointer:pointer}}
fn make<life a>(holder:&mut Holder<'a>)->impl Fn()->&i32{return move()=>holder.pointer}
fn main()->i32{
 var a=20
 var b=22
 var holder=hold(&a)
 var pointer=holder.pointer;
 if true {let f=make(&mut holder);pointer=f()}
 holder.pointer=&b
 let old=*pointer
 a=21
 return old+*holder.pointer+a-63
}
