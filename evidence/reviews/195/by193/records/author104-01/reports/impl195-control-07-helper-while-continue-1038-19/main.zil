fn run<F:Fn()->i32>(f:&F,n:i32)->i32{var i=0;var total=0;while i<n{i+=1;if i==2{continue};total+=f()}return total}fn main()->i32{let f=move()=>1;return run(&f,5)-4}
