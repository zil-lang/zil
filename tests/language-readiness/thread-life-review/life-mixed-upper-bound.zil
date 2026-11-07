fn choose<life a>(ignored:&'a i32,value:&'a mut i32)->&'a mut i32{return value}
fn main()->i32{let x=3;var y=8;let p=choose(&x,&mut y);*p+=2;return x+y-13}
