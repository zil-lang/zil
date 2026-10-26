resource Token { value:i32 }
fn consume(give token:Token) { }
fn select(flag:bool) {
    let a=Token{value:1}
    let b=Token{value:2}
    if flag { consume(give a) } else { consume(give b) }
}
fn main()->i32 { select(true); select(false); return 0 }
