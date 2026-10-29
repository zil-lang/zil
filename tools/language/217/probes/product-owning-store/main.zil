package Item{manages bytes:ByteBuffer;n:i32}
package Child{manages items:Store<Item>}
package Parent{child:Child}
fn main(){let items:Store<Item>=store_new();let p=Parent{child:Child{items:give items}}}
