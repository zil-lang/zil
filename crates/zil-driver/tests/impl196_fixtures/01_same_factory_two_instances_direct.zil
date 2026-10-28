fn make(n:i32)->impl FnMut()->i32{var value=n;return move()=>{value+=1;return value}}fn main()->i32{var left=make(1);var right=make(10);return left()+right()-13}
