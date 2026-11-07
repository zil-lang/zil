trait Touch{fn touch(self:&Self)->();}
struct A{n:i32}
struct B{n:i32}
impl Touch for A{fn touch(self:&Self)->(){return ()}}
impl Touch for B{fn touch(self:&Self)->(){return ()}}
fn touch_it(x:&dyn Touch)->(){return x.touch()}
fn main()->i32{let a=A{n:20};let b=B{n:22};touch_it(&a);touch_it(&b);return 42}
