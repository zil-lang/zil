fn make<life a>(value:&'a i32)->impl Fn()->&i32{return move()=>value}
fn main()->i32{
 var value=42
 var pointer=&value;
 {let f=make(&value);pointer=f()}
 let result=*pointer
 value=43
 return result+value-85
}
