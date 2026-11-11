target triple = "x86_64-unknown-linux-gnu"
define i32 @f(i32 %x) {
entry:
 %r = add i32 %x, 1
 ret i32 %r
}
