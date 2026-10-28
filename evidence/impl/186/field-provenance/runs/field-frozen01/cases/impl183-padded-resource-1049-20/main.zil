
extern "C" fn putchar(n:i32)->i32;
enum E{Bad(i64,i32)}
resource Wide{lead:u8;number:i64}
impl Drop for Wide{fn drop(self:Self){unsafe{putchar(i32(.lead))}}}
fn make(ok:bool)->E!Wide contract{result(create)}{if !ok{return E.Bad(123,7)};return Wide{lead:u8(65),number:123456789}}
fn main()->E!i32{let w=make(true)?;return i32(w.number-123456789)}
