import buffer

package State { manages data: ByteBuffer; sum: i64 }

// The factory transfers the whole state into an inline owning environment.
// A zero event replaces its allocated field and starts a new batch.
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
        return (state.sum + bytes_len(state.data)) % 251
    }
}
