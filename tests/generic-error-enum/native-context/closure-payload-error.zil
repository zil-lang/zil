enum Fault { Bad }
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code); } } }
extern "C" fn putchar(code:i32)->i32;
fn dispose(give token:Token) {}
fn bad()->Fault { unsafe { putchar(66); } return Fault.Bad; }
fn main()->Fault!i32 {
    let token=Token{code:65};
    let f=move(value:Fault!i32)->Fault!i32=>{
        dispose(give token);
        return value;
    };
    return f(bad());
}
