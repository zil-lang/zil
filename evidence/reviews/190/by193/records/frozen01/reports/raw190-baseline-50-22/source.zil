
fn leaf<F:Fn()->i32>(f:&F,n:i32)->i32{var i=0;var sum=0;while i<n{sum+=f();i+=1}return sum}
fn outer<F:Fn()->i32>(f:&F,n:i32)->i32{let first=leaf(f,n);let middle=f();let second=leaf(f,n+1);return first+middle+second}
fn main()->i32{let f=move()=>1;return outer(&f,3)-8}
