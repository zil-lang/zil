import allocation as generated
fn main()->void!i32 {
    let item=generated.make_generated(true)?;
    return generated.check_generated(item)
}
