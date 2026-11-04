extern "C" fn putchar(code:i32)->i32;
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code);}}}
fn worker()->i32 {
    var older=Token{code:65};
    let newer=Token{code:66};
    older=Token{code:67};
    return 0;
}
fn main()->i32{return worker();}
