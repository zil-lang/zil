extern "C" fn putchar(value:i32)->i32;

pub resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}

pub fn mark(code:i32)->Mark contract{result(create)}{
    return Mark{code:code}
}
pub fn emit(value:i32){unsafe{putchar(value)}}
