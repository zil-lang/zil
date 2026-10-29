
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R}
enum E{Fail}

package Q{manages r:R}
fn make(flag:bool)->Q!P{
 if flag{return P{r:R{code:65}}};
 return Q{r:R{code:69}}
}
fn middle(flag:bool)->Q!i32{let local=R{code:76};let p=make(flag)?;return p.r.code-65}
fn main()->Q!i32{let local=R{code:77};return middle(false)?}
