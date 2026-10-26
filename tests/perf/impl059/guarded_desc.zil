resource Token { value:i32 }
fn consume(give token:Token) { }
fn guarded(flag:bool) {
    let owner=Token{value:1}
    let description=owner
    if flag { consume(give owner) }
    if !flag { let value=description.value }
}
fn main()->i32 { return 0 }
