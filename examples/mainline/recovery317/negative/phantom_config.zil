import config
fn main()->i32 {
    return #splice(config.expression(config.Config::<i32,i64>{value:1}))
}
