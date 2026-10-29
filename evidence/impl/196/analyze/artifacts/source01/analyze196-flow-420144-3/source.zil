fn make(n:i32)->impl Fn()->i32{return move()=>n}
fn run<F:Fn()->i32>(a:&F,b:&F,n:i32)->i32{var i=0;var total=0;while i<n{total+=a();if i==1{i+=1;continue}total+=b();if i==3{break}i+=1}return total}
fn main()->i32{let a=make(1);let b=make(2);return run(&a,&b,3)-7}