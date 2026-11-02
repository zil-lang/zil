import std.meta as meta
type Recursive=#(meta.array_type(i32,i32(meta.layout(Recursive).size)))
fn main()->i32 { return 0 }
