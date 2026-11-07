fn wrong<T:Copy,life a>(value:&'a T)->T{return *value+*value}
fn main()->i32{return 0}
