struct Holder<life a>{pointer:&'a i32}
fn make<life a>(pointer:&'a i32)->Holder<'a>{return Holder::<'a>{pointer:pointer}}
fn exchange<life a>(holder:&mut Holder<'a>,next:&'a i32)->i32{
 let previous=move_out(holder.pointer)
 holder.pointer=next
 return *previous+*holder.pointer
}
fn main()->i32{
 var old=1
 var holder=make(&old)
 let result=exchange(&mut holder,&old)
 if result==2{return 0}
 return 1
}
