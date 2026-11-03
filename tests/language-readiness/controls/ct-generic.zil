fn identity<T:Copy>(x:T)->T{return x}
fn main()->i32{return #(identity(7))-7}
