fn select(right:&mut i32,left:&mut i32)->&mut i32{return &mut *left}
fn relay(x:&mut i32,y:&mut i32)->&mut i32{return select(y,x)}
fn main()->i32{var x=2;var y=9;let p=relay(&mut x,&mut y);*p+=3;return x+y-14}
