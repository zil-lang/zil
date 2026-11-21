fn same<V:Copy>(left:V,right:V)->V {
    return left
}

fn outer<T:Copy,U:Copy>(left:T,right:U)->T {
    return same(left,right)
}

fn main()->i32 {
    return outer(42,42)-42
}
