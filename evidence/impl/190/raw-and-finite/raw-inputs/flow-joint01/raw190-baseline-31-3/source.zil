
fn make(n:i32)->impl Fn()->i32{return move()=>n}
fn leaf<F:Fn()->i32>(f:&F)->i32=>f()
fn run<F:Fn()->i32>(f:&F,n:i32)->i32{let unrelated=make(99);let ignored=&unrelated;ignored;var i=0;var sum=0;while i<n{sum+=leaf(f);i+=1}return sum}
fn main()->i32{let f=make(4);return run(&f,3)-12}
