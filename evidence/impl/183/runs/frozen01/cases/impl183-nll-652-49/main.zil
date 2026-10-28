
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
enum E{Fail}

fn make()->E!R contract{result(create)}=>R{code:65}
fn wrap(give r:R)->E!R contract{result(give)}=>give r
fn main()->E!i32{let r=make()?;let old=r;let callback=()=>old.code;let n=callback();let moved=wrap(give r);return n-65}
