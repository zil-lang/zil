fn make(n:i32)->impl Fn()->i32{return move()=>n}
fn choose<F:Fn()->i32>(a:&F,b:&F,flag:bool)->i32{if flag{return a()}else{return b()}}
fn main()->i32{let a=make(2);let b=make(7);return choose(&a,&b,true)-2}