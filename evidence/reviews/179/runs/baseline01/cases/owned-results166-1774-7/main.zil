
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R}
enum E{Fail}

package Pair{manages first:R;manages second:R}
fn fail()->E!i32=>E.Fail
fn main()->E!i32{
 let u:E!Pair=Pair{first:R{code:65},second:R{code:67}};
 let guard=R{code:90};
 let prepared:E!Pair=Pair{first:R{code:66},second:R{code:fail()?}};
 return 0
}
