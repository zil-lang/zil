fn make()->impl Fn(i32)->i32{return (x:i32)->i32=>x+3}
fn dispatch<H:FnOnce(i32)->i32>(handler:H)->i32{return handler(2)}
fn run<F:FnMut(i32)->i32>(f:&mut F)->i32{
    let handler=make()
    let first=f(1)
    return dispatch(handler)+first
}
fn main()->i32{
    var f=(x:i32)->i32=>x+1
    return run(&mut f)-7
}
