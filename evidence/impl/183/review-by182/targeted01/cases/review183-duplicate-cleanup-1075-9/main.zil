extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}enum E{Fail}
fn make(ok:bool)->E!R contract{result(create)}{if ok{return R{code:65}};return E.Fail}
fn main(){let pending=make(true)}