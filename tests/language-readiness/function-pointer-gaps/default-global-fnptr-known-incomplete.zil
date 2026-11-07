fn increment(value:i32)->i32{return value+1}
trait Factory{fn handler(self:&Self)->fn(i32)->i32{return increment}}
struct Component{}
impl Factory for Component{}
fn main()->i32{
    let component=Component{};
    let handler=component.handler();
    return handler(41)-42
}
