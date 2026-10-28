
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R}
enum E{Fail}

package Data{manages data:ByteBuffer;manages marker:R}
fn make()->E!Data{
 let b=bytes_new();if !bytes_reserve(b,16)||!bytes_push(b,42u8){return E.Fail};
 return Data{data:give b,marker:R{code:65}}
}
fn main()->E!i32{let p=make()?;return i32(bytes_view(p.data)[0])-42}
