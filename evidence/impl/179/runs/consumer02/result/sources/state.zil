import model

package Batch { manages items: Store<model.Item>; manages bytes: ByteBuffer }
pub package State<F> {
    pub manages items: Store<model.Item>
    pub callback: F
    pub hits: i32
    pub manages bytes: ByteBuffer
}

fn batch() -> Batch => Batch { items: model.empty(), bytes: bytes_new() }
pub fn pack<F: FnMut() -> i32>(callback: F) -> State<F> {
    let prepared = batch()
    return State::<F> { items: give prepared.items, callback: callback, hits: 0, bytes: give prepared.bytes }
}
