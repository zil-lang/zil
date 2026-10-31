#for(width in [2]) {
    enum Event { Idle, Data(Payload), Pair(i32, bool) }
    type Payload = Packet
    struct Packet { bytes: [i32; width] }
    type Choice = Event
    fn score(event: Choice) -> i32 {
        match event {
            Choice.Idle() => { return 1 },
            Choice.Data(packet) => { return packet.bytes[0] + packet.bytes[1] },
            Choice.Pair(value, true) => { return value },
            Choice.Pair(_, false) => { return 0 }
        }
    }
    pub fn run() -> i32 {
        return (score(Choice.Idle) + score(Choice.Data(Packet { bytes: [4, 5] }))
            + score(Choice.Pair(7, true)) + score(Choice.Pair(9, false)) - 17)
    }
}
fn main() -> i32 => run()
