import allocation as generated
fn main()->void!i32 {
    let item=generated.make_generated(false)?;
    return generated.check_generated(item)
}
