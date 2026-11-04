import std.string
fn main()->string.Error!i32 {
    let input=[65u8,240u8,159u8,152u8,128u8]
    var text=string.from_utf8(&input[..],1024)?
    if string.byte_len(&text)!=5 || string.scalar_len(&text)!=2 { return 11 }
    let rune=string.scalar_at(&text,1)
    if rune.status!=1 || rune.value!=128512 || rune.next!=5 { return 12 }
    let copy=string.clone(&text,1024)?
    let extra=[66u8,67u8]
    string.append(&mut text,&extra[..],1024)?
    if string.scalar_len(&text)!=4 || string.byte_len(&copy)!=5 { return 13 }
    let replacement=[228u8,184u8,173u8]
    string.replace(&mut text,1,3,&replacement[..],1024)?
    if string.scalar_len(&text)!=3 || string.byte_len(&text)!=5 { return 14 }
    let view=string.as_bytes(&text)
    if view[0]!=65u8 || view[1]!=228u8 || view[4]!=67u8 { return 15 }
    string.clear(&mut text)
    if string.byte_len(&text)!=0 { return 16 }
    let raw=string.into_bytes(copy)
    if bytes_len(raw)!=5 { return 17 }
    return 0
}
