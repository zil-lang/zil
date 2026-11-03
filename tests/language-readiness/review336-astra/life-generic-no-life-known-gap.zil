fn bad<T:Copy>(x:&mut T)->&mut T{let a=&mut *x;let b=&mut *x;*a=*b;return x}
fn main()->i32{return 0}
