
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
enum E{Fail}

fn make(ok:bool)->E!R contract{result(create)}{if ok{return R{code:65}};return E.Fail}
fn forward(give x:E!R)->E!R contract{result(give)}{return x}
fn main()->E!i32{let guard=R{code:90};let r=forward(make(true))?;return r.code-65}
