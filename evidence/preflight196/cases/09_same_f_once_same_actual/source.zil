fn make(n:i32)->impl Fn()->i32{return move()=>n}fn pair<F:FnOnce()->i32>(left:F,right:F)->i32{return left()+right()}fn main()->i32{let f=make(1);return pair(f,f)}
