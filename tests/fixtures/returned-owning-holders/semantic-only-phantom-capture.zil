struct Marker<life a>{value:i32}
fn make<life a>(input:&'a i32)->impl Fn()->i32{
 let marker:Marker<'a>=Marker::<'a>{value:42}
 return move()=>marker.value
}
fn main()->i32{
 let value=1
 let f=make(&value)
 return f()-42
}
