import std.ui_input as input
import std.string
fn bad(session:&mut input.Composition,continued:&mut bool)->string.Error!void {
    let text=[255u8];input.update(session,&text[..],0,64)?;*continued=true;return ()
}
fn capacity(session:&mut input.Composition,document:&mut string.String,continued:&mut bool)->string.Error!void {
    let text=[65u8];input.commit(session,document,7u64,&text[..],0)?;*continued=true;return ()
}
fn main()->string.Error!i32 {
    let initial=[65u8,240u8,159u8,152u8,128u8,90u8]
    var document=string.from_utf8(&initial[..],64)?
    var session=input.new()
    input.begin(&mut session,&document,1,2,7u64)?
    let preedit=[228u8,184u8,173u8,66u8]
    input.update(&mut session,&preedit[..],1,64)?
    if !input.active(&session) || input.cursor(&session)!=1 || input.preedit(&session).len()!=4 || string.byte_len(&document)!=6{return 1}
    var continued=false
    bad(&mut session,&mut continued);capacity(&mut session,&mut document,&mut continued)
    if continued || input.cursor(&session)!=1 || input.preedit(&session).len()!=4 || string.byte_len(&document)!=6{return 2}
    let committed=[67u8]
    match input.commit(&mut session,&mut document,8u64,&committed[..],64)? {
        input.Status.Stale()=>{},_=>{return 3}
    }
    if !input.active(&session) || input.preedit(&session).len()!=4 || string.byte_len(&document)!=6{return 4}
    match input.commit(&mut session,&mut document,7u64,&committed[..],64)? {
        input.Status.Done()=>{},_=>{return 5}
    }
    if input.active(&session) || input.preedit(&session).len()!=0 || string.byte_len(&document)!=3 || string.as_bytes(&document)[1]!=67u8{return 6}
    input.begin(&mut session,&document,0,0,8u64)?
    input.update(&mut session,&preedit[..],2,64)?
    match input.cancel(&mut session){input.Status.Done()=>{},_=>{return 7}}
    if input.active(&session) || input.preedit(&session).len()!=0 || string.byte_len(&document)!=3{return 8}
    match input.cancel(&mut session){input.Status.Inactive()=>{},_=>{return 9}}
    return 0
}
