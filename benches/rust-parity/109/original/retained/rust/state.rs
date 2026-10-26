use crate::buffer;
struct State { data: Vec<u8>, sum: i64 }
pub fn make() -> impl FnMut(u8) -> i64 {
    let mut state = State { data: buffer::create(), sum: 0 };
    move |byte| {
        if byte == 0 {
            state.data = buffer::create();
            state.sum = 0;
            return 0;
        }
        if !buffer::append(&mut state.data, byte) { return -1; }
        state.sum += i64::from(byte);
        let length = state.data.len() as i64;
        let index = (state.sum + length) % length;
        let history = state.data[index as usize];
        (state.sum + length + i64::from(history)) % 251
    }
}
