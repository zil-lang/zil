type Handler=fn(i32)->i32;
fn plus(value:i32)->i32{return value+1}
fn preserve(pointer:Handler)->Handler{return pointer}
trait Factory{
 fn nested(self:&Self)->fn(Handler)->Handler{return preserve}
}
struct P{}
impl Factory for P{}
fn main()->i32{let value=P{};let pick=value.nested();let pointer=pick(plus);return pointer(41)-42}
