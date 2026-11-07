struct View<life a>{p:&'a mut i64}fn make<life a>(p:&'a mut i64)->View<'a>{return View::<'a>{p:p}}fn main(){var n:i64=40;let v=make(&mut n)}
