import std.string
import std.utf8

fn scalar_roundtrip()->i32 {
    var buffer=[77u8,77u8,77u8,77u8]
    var scalar:i32=0
    while scalar<=1114111 {
        if scalar<55296 || scalar>57343 {
            let width=utf8.encode(scalar,&mut buffer[..])
            if width<1 || width>4 { return 21 }
            let decoded=utf8.decode(&buffer[..width],0)
            if decoded.status!=1 || decoded.value!=scalar || decoded.next!=width { return 22 }
        }
        scalar+=1
    }
    buffer=[77u8,77u8,77u8,77u8]
    if utf8.encode(-1,&mut buffer[..])!=-1 || utf8.encode(55296,&mut buffer[..])!=-1 ||
       utf8.encode(57343,&mut buffer[..])!=-1 || utf8.encode(1114112,&mut buffer[..])!=-1 { return 23 }
    if utf8.encode(128512,&mut buffer[..3])!=-2 { return 24 }
    if buffer[0]!=77u8 || buffer[1]!=77u8 || buffer[2]!=77u8 || buffer[3]!=77u8 { return 25 }
    if utf8.encode(128512,&mut buffer[..])!=4 || buffer[0]!=240u8 || buffer[1]!=159u8 ||
       buffer[2]!=152u8 || buffer[3]!=128u8 { return 26 }
    return 0
}
fn invalid_scalar(text:&mut string.String, continued:&mut bool)->string.Error!void {
    string.push_scalar(text,55296,64)?
    *continued=true
    return ()
}
fn over_limit(text:&mut string.String, continued:&mut bool)->string.Error!void {
    string.push_scalar(text,65,0)?
    *continued=true
    return ()
}
fn invalid_cut(text:&mut string.String, continued:&mut bool)->string.Error!void {
    string.truncate(text,90)?
    *continued=true
    return ()
}
fn main()->string.Error!i32 {
    let scalar_check=scalar_roundtrip()
    if scalar_check!=0 { return scalar_check }
    var text=string.empty()
    if !string.is_empty(&text) { return 31 }
    string.reserve(&mut text,16,64)?
    string.push_scalar(&mut text,65,64)?
    string.push_scalar(&mut text,128512,64)?
    string.push_scalar(&mut text,20013,64)?
    string.push_scalar(&mut text,90,64)?
    if string.scalar_len(&text)!=4 || string.byte_len(&text)!=9 { return 32 }
    let middle=string.substring(&text,1,3,64)?
    if string.scalar_len(&middle)!=2 || string.byte_len(&middle)!=7 { return 33 }
    let capacity=string.capacity(&text)
    var continued=false
    invalid_scalar(&mut text,&mut continued)
    over_limit(&mut text,&mut continued)
    invalid_cut(&mut text,&mut continued)
    if continued || string.scalar_len(&text)!=4 || string.capacity(&text)!=capacity { return 34 }
    string.truncate(&mut text,2)?
    if string.byte_len(&text)!=5 || string.scalar_len(&text)!=2 { return 35 }
    string.clear(&mut text)
    if !string.is_empty(&text) || string.scalar_len(&middle)!=2 { return 36 }
    return 0
}
