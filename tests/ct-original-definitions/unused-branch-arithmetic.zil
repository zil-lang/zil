fn bad<T:Copy,#N:i32,life a>(value:T)->Type {
    if false { let sum=value+value; }
    return i32
}
fn main()->i32 { return 0 }
