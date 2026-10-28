extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}enum E{Fail}
fn make(ok:bool)->E!R contract{result(create)}{if ok{return R{code:65}};return E.Fail}
fn discard(give x:E!R)->E!R contract{result(create)}{let guard=R{code:72};let y=x?;return E.Fail}fn main()->E!i32{let guard=R{code:90};let r=discard(make(true))?;return r.code}