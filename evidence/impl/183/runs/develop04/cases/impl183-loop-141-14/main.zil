
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
enum E{Fail}

fn make(n:i32)->E!R contract{result(create)}=>R{code:n}
fn main()->i32{var i=0;while i<3{let u=make(65+i);i+=1;if i==1{continue};break};return 0}
