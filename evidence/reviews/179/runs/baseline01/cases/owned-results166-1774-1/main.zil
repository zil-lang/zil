
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R}
enum E{Fail}

fn main()->i32{var i=0;while i<3{
 let u:E!P=P{r:R{code:65+i}};i+=1;if i==1{continue};break
};return 0}
