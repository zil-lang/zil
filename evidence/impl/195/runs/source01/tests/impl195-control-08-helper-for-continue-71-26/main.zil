fn run<F:Fn(i32)->i32>(f:&F,n:i32)->i32{var total=0;for index in 0..n{if index==2{continue};total+=f(index)}return total}fn main()->i32{let f=move(index:i32)=>index;return run(&f,5)-8}
