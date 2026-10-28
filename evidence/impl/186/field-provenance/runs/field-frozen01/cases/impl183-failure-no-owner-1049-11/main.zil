
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
enum E{Fail}

fn make()->E!R contract{result(create)}=>R{code:65}
fn discard(give x:E!R)->E!R contract{result(create)}{let guard=R{code:90};return E.Fail}
fn main()->i32{discard(make());return 0}
