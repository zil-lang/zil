fn make(n:i32)->impl Fn()->i32{return move()=>n}fn choose(flag:bool)->impl Fn()->i32{if flag{return make(1)};return make(2)}fn main()->i32{let f=choose(true);return f()-1}
