resource Token { value:i32 }
fn looped(flag:bool)->i32 {
    var again=flag
    var n:i32=0
    while again {
        let item=Token{value:n}
        let read=move ()=>item.value
        n=read()
        again=false
        if flag { continue }
        break
    }
    return n
}
fn main()->i32 { return looped(true) }
