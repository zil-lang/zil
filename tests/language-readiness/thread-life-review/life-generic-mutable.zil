fn bad<T:Copy,life a>(x:&'a mut T)->&'a mut T{let a=&mut *x;let b=&mut *x;*a=*b;return x}
fn main()->i32{return 0}
