struct Box<T:Copy>{value:T}
fn select(x:Box<i32>)->Type{if x.value==7{return i32}return i64}
fn main()->i32{let n:#(select(Box::<i32>{value:7}))=7;return n-7}
