import model as m
import helper as h

fn local<T>(value:&T)->i32 where T:m.Read=>value.get()

fn main()->i32 {
    let left=m.make(17)
    let right=m.make(25)
    return local(&left)+h.relay(&right)-42
}
