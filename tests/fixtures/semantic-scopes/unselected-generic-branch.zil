fn choose<T:Copy,U:Copy>(left:T,right:U)->T {
    if true {
        return left
    } else {
        return right
    }
}

fn main()->i32 {
    return choose(42,42)-42
}
