struct View<life a>{p:&'a [i64]}fn make<life a>(p:&'a [i64])->View<'a>{return View::<'a>{p:p}}fn main()->i32{let n:[i64;2]=[20,22];let v=make(&n[..]);return i32(v.p[0]+v.p[1])}
