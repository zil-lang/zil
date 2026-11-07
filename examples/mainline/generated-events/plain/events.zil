fn duplicate<T: Copy>(value: T) -> (T, T) { return (value, value) }

type PointerPayload = [i32; 2]
struct PointerEvent { kind: i32; payload: PointerPayload }
type TouchPayload = [i32; 3]
struct TouchEvent { kind: i32; payload: TouchPayload }

fn pointer_score(event: PointerEvent) -> i32 {
    var total = event.kind
    for i in 0..2 { total += event.payload[i] }
    return total
}
fn touch_score(event: TouchEvent) -> i32 {
    var total = event.kind
    for i in 0..3 { total += event.payload[i] }
    return total
}
pub fn pointer_event() -> i32 {
    let pair = duplicate(PointerEvent { kind: 65, payload: [4, 5] })
    if pointer_score(pair.0) != pointer_score(pair.1) { return 0 }
    return pointer_score(pair.1)
}
pub fn touch_event() -> i32 {
    let pair = duplicate(TouchEvent { kind: 66, payload: [6, 7, 8] })
    if touch_score(pair.0) != touch_score(pair.1) { return 0 }
    return touch_score(pair.1)
}
