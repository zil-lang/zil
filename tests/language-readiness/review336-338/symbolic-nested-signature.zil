fn preserve(pointer:fn(i32)->i32)->fn(i32)->i32{return pointer}
trait Factory<R:Copy>{
 fn nested(self:&Self)->fn(fn(R)->R)->fn(R)->R{return preserve}
}
fn main(){}
