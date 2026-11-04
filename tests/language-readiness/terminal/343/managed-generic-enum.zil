extern "C" fn putchar(code:i32)->i32;
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code);}}}
enum Envelope<T>{Value(T),Empty}
fn main()->i32{
    let token=Token{code:65};
    let value:Envelope<Managed<Token>>=Envelope.Value(give token);
    match value {
        Envelope.Value(owner)=>{return 0;},
        Envelope.Empty()=>{return 1;}
    }
}
