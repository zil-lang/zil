import factory
import helpers
import state
fn main()->i32 {
    let snapshot=factory.snapshot(65u8)
    let first=helpers.peek(&snapshot)
    helpers.emit(i32(first[0]))
    let current=state.make(factory.grow())
    let answer=state.run(current,3)
    if answer!=0{return answer}
    let second=helpers.peek(&snapshot)
    helpers.emit(i32(second[0]))
    return i32(first[0])-i32(second[0])
}