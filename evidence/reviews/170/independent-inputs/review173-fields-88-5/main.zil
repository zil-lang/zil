
extern "C" fn putchar(x:i32)->i32;
resource Empty {}
impl Drop for Empty { fn drop(self:Self) { unsafe { putchar(65) } } }
package Leaf { manages h:Empty }
package State { manages tag:Empty; items:[Leaf;2] }
fn fresh()->Leaf=>Leaf{h:Empty{}}
fn state()->State=>State{tag:Empty{},items:[fresh(),fresh()]}
 fn main()->i32{var s=state();var n=0;while n<4{let a=s.items;s.items=a;n+=1};return 0}