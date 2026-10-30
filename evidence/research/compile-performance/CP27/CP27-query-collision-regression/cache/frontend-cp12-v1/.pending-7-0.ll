; ZIL verified LLVM module
; verified-fingerprint: 0e8268737d19c798a03545d26a76f6bce68f3a4551dfe2fbec3ca0c5b26a93d2
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
define i32 @zil.fn.0() {
entry:
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  ret i32 %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
