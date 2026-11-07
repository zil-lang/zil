pub trait Factory{type Item;}
pub fn relay<T:Factory>(value:T.Item)->T.Item{return value}
