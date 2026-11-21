
fn make<life a>(pointer:&'a i32)->impl Fn()->i32{
 let inner=move()=>*pointer
 return move()=>inner()
}
fn main()->i32{let value=41;let outer=make(&value);return outer()-41}
