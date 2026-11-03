fn first(left:&mut i32,right:&mut i32)->&mut i32{return left}
fn main()->i32{var x=5;var y=7;let p=first(&mut x,&mut y);y=11;*p=9;return x+y-20}
