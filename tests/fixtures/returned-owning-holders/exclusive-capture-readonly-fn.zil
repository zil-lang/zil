fn make(value:&mut i32)->impl Fn()->i32{return move()=>*value} fn main()->i32{var n=21;let f=make(&mut n);let a=f();let b=f();return a+b-42}
