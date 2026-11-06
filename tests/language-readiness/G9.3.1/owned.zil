import std.string
fn main()->string.Error!i32 {
    var text=string.from_utf8("按钮🙂",64)?
    if string.byte_len(&text)!=10 || string.scalar_len(&text)!=3 { return 11 }
    let copy=string.clone(&text,64)?
    string.append(&mut text,"\0Z",64)?
    if string.byte_len(&text)!=12 || string.byte_len(&copy)!=10 { return 12 }
    string.replace(&mut text,0,1,"A",64)?
    if string.byte_len(&text)!=10 || string.as_bytes(&text)[0]!=65u8 { return 13 }
    let empty=string.from_utf8("",0)?
    if string.byte_len(&empty)!=0 || string.capacity(&empty)!=0 { return 14 }
    return 0
}
