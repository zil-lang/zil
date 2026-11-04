extern "C" fn putchar(code:i32)->i32;
resource Token {code:i32}
impl Drop for Token {fn drop(self:Self){unsafe{putchar(.code);}}}
enum Fault{Stop}
fn failed()->Fault!i32 {return Fault.Stop;}
fn combine(give token:Token,value:i32)->i32{return value;}
fn prepare(give a:Token,give b:Token)->Fault!i32 {
    return combine(give a,failed()?);
}
fn main()->i32 {
    let a=Token{code:65};let b=Token{code:66};
    prepare(give a,give b);
    return 0;
}
