fn make(n:i32)->impl Fn()->i32{return move()=>n}
fn pair<F:FnOnce()->i32>(left:F,right:F)->i32{return left()+right()}
fn main()->i32{let left=make(1);let right=make(2);return pair(left,right)-3}