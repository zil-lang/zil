
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R}
enum E{Fail}

fn make()->E!P=>P{r:R{code:65}}
fn next(ok:bool)->E!i32{if ok{return 0};return E.Fail}
fn consume(x:E!P,n:i32)->i32{unsafe{putchar(67)};return n}
fn main()->E!i32{let guard=R{code:90};return consume(make(),next(false)? )}
