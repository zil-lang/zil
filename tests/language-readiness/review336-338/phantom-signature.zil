struct Tag<A:Copy,B:Copy>{}
fn preserve(value:Tag<i32,bool>)->Tag<i32,bool>{return value}
trait Factory{
 fn nested(self:&Self)->fn(Tag<bool,i32>)->Tag<bool,i32>{return preserve}
}
fn main(){}
