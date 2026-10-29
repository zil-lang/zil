extern "C" fn putchar(x:i32)->i32;

pub resource Counter{code:i32;pub n:i32}
impl Drop for Counter{fn drop(self:Self){unsafe{putchar(.code)}}}

pub fn make(code:i32,n:i32)->Counter contract{result(create)}{
    return Counter{code:code,n:n}
}
