import extra_generators as extra
fn main()->i32 {
    let private_local=7;
    return #splice(extra.exported_name_is_not_input())
}
