extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}enum E{Fail}
fn make(ok:bool)->E!R contract{result(create)}{if ok{return R{code:65}};return E.Fail}
fn select(give incoming:E!R,give spare:R,choose:bool)->E!R contract{result(give)}{let local=R{code:76};if choose{return incoming};return give spare}fn main()->E!i32{let guard=R{code:90};let r=select(make(true),R{code:66},false)?;return 0}