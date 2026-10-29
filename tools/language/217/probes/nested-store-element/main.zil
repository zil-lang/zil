package Item{manages bytes:ByteBuffer;n:i32}
package State{manages items:Store<Item>;tag:i32}
fn main(){let states:Store<State>=store_new()}
