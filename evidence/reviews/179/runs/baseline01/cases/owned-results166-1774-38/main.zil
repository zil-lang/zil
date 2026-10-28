
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R}
enum E{Fail}

package Q{manages r:R}
fn success()->Q!P=>P{r:R{code:65}}
fn failure()->Q!P=>Q{r:R{code:69}}
fn main()->i32{success();failure();let a=success();let b=failure();return 0}
