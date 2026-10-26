resource Token { value:i32 }

fn bundle2(flag:bool)->i32 {
    let a=Token{value:1}
    let b=Token{value:2}
    let read=move ()=>a.value+b.value
    if flag { return read() }
    return 0
}

fn bundle6(flag:bool)->i32 {
    let a=Token{value:1}
    let b=Token{value:2}
    let c=Token{value:3}
    let d=Token{value:4}
    let e=Token{value:5}
    let f=Token{value:6}
    let read=move ()=>a.value+b.value+c.value+d.value+e.value+f.value
    if flag { return read() }
    return 0
}

fn main()->i32 { return bundle2(true)+bundle6(false) }
