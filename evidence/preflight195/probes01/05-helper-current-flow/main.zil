fn run<F:Fn()->i32>(f:&F,n:i32)->i32{var i=0;var total=0;while i<n{total+=f();i+=1}return total}fn main()->i32{let f=move()=>3;return run(&f,3)-9}
