fn select<T:Copy>(x:T)->Type{return T}
fn main()->i32{let n:#(select(7))=7;return n-7}
