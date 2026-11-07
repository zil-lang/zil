fn choose(ignored:&i32,value:&mut i32)->&mut i32{return value}
fn main()->i32{let x=3;var y=8;let p=choose(&x,&mut y);*p+=2;return x+y-13}
