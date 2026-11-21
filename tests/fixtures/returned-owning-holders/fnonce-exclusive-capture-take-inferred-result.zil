fn make<life a>(value:&'a mut i32)->impl FnOnce()->&mut i32{
 return move()=>give value
}
fn main()->i32{
 var value=20
 let f=make(&mut value)
 let pointer=f()
 *pointer+=2
 return *pointer-22
}
