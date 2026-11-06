fn text()->&[u8] { return "按钮🙂" }
fn relay(value:&[u8])->&[u8] { return value }
fn checksum(value:&[u8])->i64 {
    var result:i64=0
    var at:i64=0
    while at<value.len() { result=result*17+i64(value[at]); at+=1 }
    return result
}
fn result()->i64 {
    let empty=""
    if empty.len()!=0 { return -1 }
    if "a"[0]!=97u8 || "按钮".len()!=6 { return -2 }
    let tail=&text()[3..]
    if tail.len()!=7 || tail[0]!=233u8 { return -3 }
    let combining="e\u{301}"
    if combining.len()!=3 || "é".len()!=2 { return -4 }
    return checksum(relay(text()))+checksum("a\0b")+checksum(combining)
}
