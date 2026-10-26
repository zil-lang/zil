fn bump(p: &mut i32) { *p = *p + 1; }
fn answer() -> i32 {
    var x = 40;
    bump(&mut x);
    let p = &mut x;
    let q = &mut *p;
    *q = *q + 1;
    return x;
}
