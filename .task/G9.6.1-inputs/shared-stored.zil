struct View<life a> { p: &'a i64 }
fn make<life a>(p: &'a i64) -> View<'a> {
    return View::<'a> { p: p };
}
fn read<life a>(v: &View<'a>) -> &'a i64 { return v.p; }
fn main() -> i32 {
    let n: i64 = 42;
    let v = make(&n);
    let q = read(&v);
    return i32(*q);
}
