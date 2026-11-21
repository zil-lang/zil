fn make(value:&mut i32)->impl FnMut()->&i32{
 var calls=0
 return move()=>{calls+=1;return &*value}
}
fn main()->i32{
 var value=21
 var f=make(&mut value)
 let first=*f()
 let view=f()
 let second=*view
 let third=*f()
 return first+second+third-63
}
