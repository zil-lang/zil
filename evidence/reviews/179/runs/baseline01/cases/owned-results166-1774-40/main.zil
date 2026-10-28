
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R}
enum E{Fail}

fn main()->i32{var v:E!P=P{r:R{code:65}};let later=R{code:90};v=P{r:R{code:66}};return 0}
