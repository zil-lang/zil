
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
enum E{Fail}

fn success()->E!R contract{result(create)}=>R{code:65}
fn failure()->E!R contract{result(create)}=>E.Fail
fn main()->i32{success();failure();let a=success();let b=failure();return 0}
