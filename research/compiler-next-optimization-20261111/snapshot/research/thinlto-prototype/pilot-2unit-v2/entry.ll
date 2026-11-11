target triple = "x86_64-unknown-linux-gnu"
declare i32 @f(i32)
define i32 @main() {
entry:
 %r = call i32 @f(i32 41)
 ret i32 %r
}
