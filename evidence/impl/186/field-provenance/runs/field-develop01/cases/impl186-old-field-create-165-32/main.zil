
extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R;pub n:i32}
package Q{lead:u8;manages r:R;pub wide:i64}
enum E{Fail}
fn p(code:i32)->P=>P{r:R{code:code},n:7}
fn q(code:i32)->Q=>Q{lead:u8(3),r:R{code:code},wide:123456789}
package Pair{manages old:R;manages fresh:R}fn bad(give old:R)->R contract{result(create)}{let pair=Pair{old:give old,fresh:R{code:66}};let moved=pair;return give moved.old}fn main(){}