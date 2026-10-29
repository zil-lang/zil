pub fn step<F:Fn()->i32>(f:&F)->i32{
    return f()
}
