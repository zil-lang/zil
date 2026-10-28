extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Leaf{manages a:R;pad:[i32;2];manages b:R}
package Twins{left:Leaf;middle:i64;right:Leaf}
package Root{branch:Twins;manages tail:R}
fn leaf(a:i32,b:i32)->Leaf{return Leaf{a:R{code:a},pad:[a,b],b:R{code:b}}}
fn root()->Root{return Root{branch:Twins{left:leaf(65,66),middle:123i64,right:leaf(67,68)},tail:R{code:90}}}
fn relay(value:Root)->Root{return value}
fn main()->i32{let value=relay(root());let twins=value.branch;let middle=twins.middle;let left=twins.left;let right=twins.right;return left.pad[0]+left.b.code+right.pad[1]+right.a.code+i32(middle)-389}
