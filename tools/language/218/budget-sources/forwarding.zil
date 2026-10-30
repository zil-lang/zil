fn make(n:i32)->impl Fn(i32)->i32{return move(x:i32)=>n+x}
pub fn wrap<F:FnOnce(i32)->i32>(f:F,bias:i32)->impl FnOnce(i32)->i32{return move(x:i32)=>{let nested=move(y:i32)=>f(y)+bias;return nested(x)}}
pub fn outer<G:FnOnce(i32)->i32>(g:G)->impl FnOnce(i32)->i32{return wrap(g,2)}
fn main()->i32{let output=outer(make(36));return output(4)-42}
