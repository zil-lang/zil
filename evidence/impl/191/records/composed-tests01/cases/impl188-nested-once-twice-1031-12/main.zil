fn leaf<F:FnOnce()->i32>(f:F)->i32=>f()
fn run<F:FnOnce()->i32>(f:F)->i32=>leaf(f)+leaf(f)
fn main(){}