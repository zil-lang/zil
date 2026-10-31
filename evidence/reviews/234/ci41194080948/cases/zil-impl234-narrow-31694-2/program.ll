; ZIL verified LLVM module
; verified-fingerprint: bc23795c5a40847707fad96026315872f65561a2e76e0ce282afd072408f1d71
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)
declare i32 @getchar() nonlazybind
define i32 @zil.fn.1(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i1, align 1
  %local.2 = alloca i32, align 4
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
  store i32 %v.9, ptr %local.2, align 4
  br label %bb.7
bb.7:
  %v.10 = load i32, ptr %local.2, align 4
  %v.11 = add i32 0, 0
  %v.12 = icmp sgt i32 %v.10, %v.11
  br i1 %v.12, label %bb.8, label %bb.9
bb.8:
  %v.13 = load i32, ptr %local.2, align 4
  %v.14 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.13, i32 %v.14)
  %v.15 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.15, ptr %local.2, align 4
  br label %bb.7
bb.9:
  %v.16 = add i32 0, 0
  ret i32 %v.16
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
