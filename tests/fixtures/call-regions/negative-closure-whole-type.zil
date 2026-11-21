fn bad<T>(value:T)->T {
    let identity=(argument:T)=>argument;
    return identity(true)
}
fn main()->i32{return 0}
