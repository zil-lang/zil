
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R}
enum E{Fail}

fn make()->E!P{let p=P{r:R{code:65}};return p}
fn identity(x:E!P)->E!P=>x
fn forward(x:E!P)->E!P=>identity(x)
fn main()->E!i32{let p=forward(make())?;return p.r.code-65}
