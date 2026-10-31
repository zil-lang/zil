; ZIL verified LLVM module
; verified-fingerprint: 81f4390fc198342dc2c23729d206f6d752961c924162a75962ada9d1a59c0964
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare i32 @getchar() nonlazybind
define i32 @zil.fn.1(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i1, align 1
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = add i32 0, 0
  %v.2 = icmp slt i32 %v.0, %v.1
  br i1 %v.2, label %bb.1, label %bb.2
bb.1:
  %v.3 = or i1 false, true
  store i1 %v.3, ptr %local.1, align 1
  br label %bb.3
bb.2:
  %v.4 = load i32, ptr %local.0, align 4
  %v.5 = add i32 0, 64
  %v.6 = icmp sgt i32 %v.4, %v.5
  store i1 %v.6, ptr %local.1, align 1
  br label %bb.3
bb.3:
  %v.7 = load i1, ptr %local.1, align 1
  br i1 %v.7, label %bb.4, label %bb.5
bb.4:
  %v.8 = add i32 0, -2
  ret i32 %v.8
bb.5:
  br label %bb.6
bb.6:
  %v.9 = load i32, ptr %local.0, align 4
  %v.10 = add i32 0, 0
  %v.11 = icmp eq i32 %v.9, %v.10
  br i1 %v.11, label %bb.7, label %bb.8
bb.7:
  %v.12 = add i32 0, 0
  ret i32 %v.12
bb.8:
  br label %bb.9
bb.9:
  %v.13 = add i32 0, -1
  ret i32 %v.13
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.2() {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = call i32 @getchar()
  store i32 %v.1, ptr %local.0, align 4
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = call i32 @zil.fn.1(i32 %v.2)
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.2()
  ret i32 %entry.result
}
