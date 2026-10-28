fn leaf<F:Fn()->i32>(f:&F)->i32=>f()
fn run<F:Fn()->i32>(f:&F)->i32{var n=0;while n<3{n+=leaf(f)};return n}
fn main()->i32{let f=move()=>1;return run(&f)}