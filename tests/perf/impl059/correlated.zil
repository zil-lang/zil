resource Token { value:i32 }
fn consume(give token:Token) { }
fn correlated(flag:bool) {
    let a=Token{value:1}
    let b=Token{value:2}
    if flag { consume(give a) } else { consume(give b) }
    if flag { consume(give b) } else { consume(give a) }
}
fn main()->i32 { correlated(true); correlated(false); return 0 }
