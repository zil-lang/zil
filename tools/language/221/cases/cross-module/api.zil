pub fn wrap<F:Fn(i32)->i32>(bias:i32,f:F)->impl Fn(i32)->i32{
    return move(x:i32)=>f(x)+bias
}
