pub fn wrap<F:Fn(i32)->i32>(f:F,bias:i32)->impl Fn(i32)->i32 {
    return move (x:i32)=>f(x)+bias
}
pub fn once<F:FnOnce(i32)->i32>(f:F)->impl FnOnce(i32)->i32 {
    return move (x:i32)=>f(x)
}
pub fn mutable<F:FnMut(i32)->i32>(f:F)->impl FnMut(i32)->i32 {
    var inner=f
    return move (x:i32)=>inner(x)
}
