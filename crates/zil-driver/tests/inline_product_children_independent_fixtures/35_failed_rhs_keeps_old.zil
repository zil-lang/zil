extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Leaf{manages a:R;pad:[i32;2];manages b:R}
package Twins{left:Leaf;middle:i64;right:Leaf}
package Root{branch:Twins;manages tail:R}
fn leaf(a:i32,b:i32)->Leaf{return Leaf{a:R{code:a},pad:[a,b],b:R{code:b}}}
fn root()->Root{return Root{branch:Twins{left:leaf(65,66),middle:i64(123),right:leaf(67,68)},tail:R{code:90}}}
fn relay(value:Root)->Root{return value}
enum E{Fail}fn fail()->E!i32{return E.Fail}fn main()->E!i32{var value=root();value.branch=Twins{left:leaf(69,70),middle:i64(321),right:leaf(fail()?,72)};return 0}
