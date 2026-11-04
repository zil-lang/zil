extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code); } } }
fn dispose(give token:Token) { }
fn work(flip:bool)->i32 {
    var a=Token{code:65};
    var b=Token{code:66};
    if flip {
        a=Token{code:67};
        b=Token{code:68};
    } else {
        b=Token{code:68};
        a=Token{code:67};
    }
    if flip {
        dispose(give b);
        dispose(give a);
    } else {
        dispose(give a);
        dispose(give b);
    }
    return 0;
}
fn main()->i32 { work(false); work(true); return 0; }
