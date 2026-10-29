
extern "C" fn putchar(n:i32)->i32;
resource Empty{}
impl Drop for Empty{fn drop(self:Self){unsafe{putchar(90)}}}
fn make()->void!Empty contract{result(create)}=>Empty{}
fn main()->void!i32{let e=make()?;return 0}
