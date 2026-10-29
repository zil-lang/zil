pub fn four<F:FnMut(i64)->bool!i64>(f:&mut F)->bool!i64{let a=f(1)?;let b=f(2)?;let c=f(3)?;let d=f(4)?;return a+b+c+d}
