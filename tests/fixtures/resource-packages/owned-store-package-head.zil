package String{manages data:ByteBuffer}
package TextEntry{key:String;value:i64}
package StringMap{manages entries:Store<TextEntry>}
fn length(map:&StringMap)->i64{return store_len(map.entries)}
fn fail(map:&mut StringMap,key:String)->TextEntry!i32{return TextEntry{key:key,value:7}}
fn main()->i32{let key=String{data:bytes_new()};var map=StringMap{entries:store_new()};match fail(&mut map,key){.ok(_)=>{return 0},.err(_)=>{return 1}}}
