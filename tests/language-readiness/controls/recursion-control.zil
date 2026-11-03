fn sum(n:i32)->i32{if n==0{return 0}return n+sum(n-1)}
fn main()->i32{return sum(3)-6}
