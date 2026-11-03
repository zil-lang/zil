resource R{}
fn preserve(pointer:fn(R)->R)->fn(R)->R{return pointer}
trait Factory{
 fn nested(self:&Self)->fn(fn(R)->R)->fn(R)->R{return preserve}
}
fn main(){}
