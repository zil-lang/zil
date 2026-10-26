struct View { target: &mut i32; label: i32 }
fn changed(value: View) -> View {
    return View { ...value, label: 2 }
}
