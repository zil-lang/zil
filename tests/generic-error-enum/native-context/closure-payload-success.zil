enum Fault { Bad }
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code); } } }
extern "C" fn putchar(code:i32)->i32;
fn next(p:&mut i32)->i32 { *p+=1;return 7; }
fn main()->Fault!i32 {
    let token=Token{code:65};
    var calls=0;
    var f=(value:Fault!i32, later:i32)->Fault!i32=>{
        calls+=1;
        return value?+later+token.code-65;
    };
    var count=0;
    let value=f(next(&mut count),count)?;
    return value+count+calls-10;
}
