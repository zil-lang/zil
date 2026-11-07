fn duplicate<T: Copy>(value: T) -> (T, T) { return (value, value) }

#for (lanes in [2, 3]) {
    type Payload = [i32; lanes]
    struct Event { kind: i32; payload: Payload }

    fn score(event: Event) -> i32 {
        var total = event.kind
        for i in 0..lanes { total += event.payload[i] }
        return total
    }
    #if (lanes == 2) {
        pub fn pointer_event() -> i32 {
            let pair = duplicate(Event { kind: 65, payload: [4, 5] })
            if score(pair.0) != score(pair.1) { return 0 }
            return score(pair.1)
        }
    } else {
        pub fn touch_event() -> i32 {
            let pair = duplicate(Event { kind: 66, payload: [6, 7, 8] })
            if score(pair.0) != score(pair.1) { return 0 }
            return score(pair.1)
        }
    }
}
