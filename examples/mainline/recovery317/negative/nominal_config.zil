import config
struct Other<T:Copy> { pub value:T }
fn main()->i32 {
    return #splice(config.expression(Other::<i32>{value:1}))
}
