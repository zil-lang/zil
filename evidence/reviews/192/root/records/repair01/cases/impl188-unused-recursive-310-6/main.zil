fn left<F:Fn()->i32>(f:&F)->i32=>right(f);fn right<F:Fn()->i32>(f:&F)->i32=>left(f)
fn main(){}