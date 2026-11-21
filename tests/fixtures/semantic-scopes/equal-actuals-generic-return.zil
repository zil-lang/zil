fn bad<T:Copy,U:Copy>(value:T)->U {
    return value
}

fn main()->i32 {
    return bad::<i32,i32>(42)-42
}
