fn select<life b,life a>(right:&'b mut i32,left:&'a mut i32)->&'a mut i32{return &mut *left}
fn relay<life left,life right>(x:&'left mut i32,y:&'right mut i32)->&'left mut i32{return select(y,x)}
fn main()->i32{var x=2;var y=9;let p=relay(&mut x,&mut y);*p+=3;return x+y-14}
