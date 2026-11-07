enum Event { Key(i32), Empty }

#for (once in [0]) {
    type Alias = Event
    fn decode(value: Alias) -> i32 {
        match value {
            Alias.Key(code) => { return code },
            Alias.Empty() => { return 0 }
        }
    }
    pub fn entry() -> i32 { return decode(Event.Key(7)) - 7 }
}
fn main() -> i32 { return entry() }
