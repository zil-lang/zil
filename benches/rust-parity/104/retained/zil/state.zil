import buffer
package State { manages data: ByteBuffer; sum: i64 }
pub fn make() -> impl FnMut(u8) -> i64 {
    var state = State { data: buffer.create(), sum: i64(0) }
    return move(byte) => {
        if byte == 0u8 {
            state.data = buffer.create()
            state.sum = 0
            return i64(0)
        }
        if !buffer.append(state.data, byte) { return i64(-1) }
        state.sum += i64(byte)
        let length = bytes_len(state.data)
        let index = (state.sum + length) % length
        let history = buffer.view(state.data)[index]
        return (state.sum + length + i64(history)) % 251
    }
}
