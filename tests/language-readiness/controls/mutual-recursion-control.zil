fn even(n:i32)->bool{if n==0{return true}return odd(n-1)}
fn odd(n:i32)->bool{if n==0{return false}return even(n-1)}
fn main()->i32{if even(4){return 0}return 1}
