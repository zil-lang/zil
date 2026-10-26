resource Token { value:i32 }
fn consume(give token:Token) { }
fn independent(a:bool,b:bool,c:bool,d:bool) {
    let r0=Token{value:0}
    let r1=Token{value:1}
    let r2=Token{value:2}
    let r3=Token{value:3}
    if a { consume(give r0) }
    if b { consume(give r1) }
    if c { consume(give r2) }
    if d { consume(give r3) }
}
fn main()->i32 { independent(true,false,true,false); return 0 }
