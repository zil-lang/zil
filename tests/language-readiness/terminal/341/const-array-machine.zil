fn fixed<T:Copy,#N:i32>(x:[T;N])->[T;N]{return x}fn result()->i32{let a=fixed::<i32,2>([7,9]);let b=fixed::<i32,1>([a[0]]);return b[0]}
