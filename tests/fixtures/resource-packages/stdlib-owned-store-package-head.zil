import std.string
pub package TextEntry { pub key:string.String; pub value:i64 }
pub package StringMap { manages entries:Store<TextEntry> }
fn fail(map:&mut StringMap,key:string.String)->TextEntry!i32 {
    return TextEntry{key:key,value:7}
}
fn main()->i32 {
    let key=string.empty()
    var map:StringMap=StringMap{entries:store_new()}
    match fail(&mut map,key) {
        .ok(_)=>{return 0},
        .err(_)=>{return 1}
    }
}
