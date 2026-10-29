fn leaf<F:Fn()->i32>(f:&F)->i32=>f()
fn run<F:FnMut()->i32>(f:&mut F)->i32=>leaf(f)
fn main(){}