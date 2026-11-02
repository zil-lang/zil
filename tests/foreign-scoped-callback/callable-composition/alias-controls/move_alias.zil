fn run<F:Fn()->i32>(f:F)->i32{let local=()->i32=>5;let alias=f;return alias()+alias()+local()}fn main()->i32{let f=()->i32=>10;return run(f)-25}
