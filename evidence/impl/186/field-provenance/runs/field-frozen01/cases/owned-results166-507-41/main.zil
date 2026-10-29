
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R}
enum E{Fail}

package Data{manages data:ByteBuffer;manages marker:R}
fn make_error()->Data!i32{
 let b=bytes_new();if !bytes_reserve(b,16)||!bytes_push(b,42u8){return 91};
 return Data{data:give b,marker:R{code:65}}
}
fn main()->Data!i32{let local=R{code:77};return make_error()?}
