
extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R;pub n:i32}
package Q{lead:u8;manages r:R;pub wide:i64}
enum E{Fail}
fn p(code:i32)->P=>P{r:R{code:code},n:7}
fn q(code:i32)->Q=>Q{lead:u8(3),r:R{code:code},wide:123456789}
fn create(flag:bool)->R contract{result(create)}{var slot:R=R{code:65};if flag{slot=R{code:66}}else{slot=R{code:67}};return give slot}fn main()->i32{let r=create(false);return 0}