struct View { target: &mut i32; label: i32 }
fn bad(value: View) -> i32 {
    let changed = View { ...value, label: 2 }
    return value.label
}
