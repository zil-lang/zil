fn triple(value:i32)->i32{return value*3}
pub trait Factory{fn select(self:&Self)->fn(i32)->i32{return triple}}
pub fn select<T:Factory>(value:&T)->fn(i32)->i32{return value.select()}
