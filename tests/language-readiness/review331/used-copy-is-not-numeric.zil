fn wrong<life a,T:Copy>(value:&'a T)->T{return *value+*value}
fn main()->i32{let value=7;return wrong::<i32>(&value)-14}
