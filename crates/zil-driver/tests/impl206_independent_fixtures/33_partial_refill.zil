extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Leaf{manages a:R;pad:[i32;2];manages b:R}
package Twins{left:Leaf;middle:i64;right:Leaf}
package Root{branch:Twins;manages tail:R}
fn leaf(a:i32,b:i32)->Leaf{return Leaf{a:R{code:a},pad:[a,b],b:R{code:b}}}
fn root()->Root{return Root{branch:Twins{left:leaf(65,66),middle:i64(123),right:leaf(67,68)},tail:R{code:90}}}
fn relay(value:Root)->Root{return value}
fn main()->i32{var value=root();var twins=value.branch;var left=twins.left;let old=give left.a;left.a=R{code:69};twins.left=left;value.branch=twins;return old.code-65}
