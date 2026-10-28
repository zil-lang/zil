fn run<F:Fn()->i32>(f:&F)->i32=>run(f)
fn main()->i32{let f=move()=>7;return run(&f)}