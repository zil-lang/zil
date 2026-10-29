
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
enum E{Fail}

fn make(ok:bool)->E!R contract{result(create)}{if ok{return R{code:65}};return E.Fail}
fn middle(ok:bool)->E!R contract{result(create)}{let local=R{code:76};let r=make(ok)?;return give r}
fn main()->E!i32{let local=R{code:77};let pending=middle(true);let r=pending?;return r.code-65}
