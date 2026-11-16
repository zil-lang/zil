fn keep<T>(value:T)->T {
    let identity=(argument:T)=>argument;
    return identity(value)
}
fn main()->i32 {
    let read=(pointer:&i32)=>*pointer;
    var left=17;
    var right=23;
    let first=keep(&left);
    return read(first)+read(&right)-40
}
