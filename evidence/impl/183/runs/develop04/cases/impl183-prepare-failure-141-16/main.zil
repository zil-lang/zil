
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
enum E{Fail}

package Pair{manages a:R;manages b:R}
fn make()->E!R contract{result(create)}=>R{code:65}
fn fail()->E!i32=>E.Fail
fn main()->E!i32{let u=make();let guard=R{code:90};let p=Pair{a:R{code:66},b:R{code:fail()?}};return 0}
