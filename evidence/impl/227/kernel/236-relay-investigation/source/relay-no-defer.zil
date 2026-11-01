extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){emit(.code)}}
package Owner{manages marker:Mark}
fn relay(owned:Owner)->Owner{return owned}
fn main()->i32{
var current=Owner{marker:Mark{code:65}}
let late=Mark{code:76}
current=relay(current)
return 0
}
