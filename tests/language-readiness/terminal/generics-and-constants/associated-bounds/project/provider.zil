import api

pub struct Box<T:Copy>{value:T}
pub struct Number{pub value:i32}

impl api.Marker<Number> for Number{}

impl<T:Copy+api.Marker<T>> api.Read<i32> for Box<T>{
    type Item:api.Marker<T>=T;
    fn read()->Self.Item{return .value}
}

pub fn make(value:i32)->Box<Number>{return Box::<Number>{value:Number{value:value}}}
pub fn number(value:Number)->i32{return value.value}
