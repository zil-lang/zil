fn identity<T:Copy,life a>(value:&'a T)->&'a T{return value}
fn main()->i32{let value=7;return *identity(&value)-7}
