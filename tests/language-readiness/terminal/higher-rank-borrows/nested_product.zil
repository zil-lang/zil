import std.string
package Composition { text:string.String; active:bool }
fn begin(session:&mut Composition) { string.clear(&mut session.text);session.active=true }
fn preedit(session:&Composition)->&[u8] { return string.as_bytes(&session.text) }
fn main()->string.Error!i32 {
    let initial=[65u8,66u8]
    var session=Composition{text:string.from_utf8(&initial[..],64)?,active:false}
    if preedit(&session).len()!=2{return 1}
    begin(&mut session)
    if !session.active || preedit(&session).len()!=0{return 2}
    return 0
}
