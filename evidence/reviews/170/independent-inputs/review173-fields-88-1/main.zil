
resource R{code:i32}
package Item{manages r:R;manages data:ByteBuffer}
package State{manages r:R;items:[Item;2]}
fn item()->Item{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));return Item{r:R{code:0},data:give b}}
fn main()->i32{var s=State{r:R{code:1},items:[item(),item()]};let a=s.items;let view=bytes_view(a[0].data);s.items=a;return i32(view[0])-65}
