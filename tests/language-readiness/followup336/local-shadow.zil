fn increment(value:i32)->i32{return value+1}
fn alternate(value:i32)->i32{return value+2}
trait Factory{
    fn forwarded(self:&Self,increment:fn(i32)->i32)->fn(i32)->i32{return increment}
    fn local(self:&Self)->fn(i32)->i32{let increment=alternate;return increment}
}
struct Component{}
impl Factory for Component{}
fn main()->i32{
    let component=Component{};
    let forwarded=component.forwarded(increment);
    let local=component.local();
    if forwarded(41)!=42{return 1}
    return local(40)-42
}
