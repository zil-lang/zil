; ZIL verified LLVM module
; verified-fingerprint: 6be8fd71e1f22a581cf07b51f3d970b4a87599504faa4e453a5929041da2f0ba
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
