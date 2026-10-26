resource Token { value:i32 }
fn consume(give token:Token)->i32 { return token.value }
fn owned(flag:bool)->i32 {
    let a=Token{value:1}
    let b=Token{value:2}
    let read=move ()=>a.value+b.value
    if flag { return read() }
    return 0
}
fn once()->i32 {
    let a=Token{value:3}
    let b=Token{value:4}
    let run=move ()=>{ consume(give a); b.value }
    return run()
}
fn main()->i32 { return owned(true)+once() }
