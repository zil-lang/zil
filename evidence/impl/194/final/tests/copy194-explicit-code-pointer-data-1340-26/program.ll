; ZIL verified LLVM module
; verified-fingerprint: c9a69830b230a1d9b84f1fc3156c63f8586e4c2e68afe393676481614ed8d17a
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = freeze ptr @zil.fn.2
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %v.2 = call ptr @zil.fn.1(ptr %v.1)
  store ptr %v.2, ptr %local.1, align 8
  %v.3 = load ptr, ptr %local.1, align 8
  %v.4 = call i32 %v.3()
  %v.5 = add i32 0, 7
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define ptr @zil.fn.1(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  ret ptr %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2() {
entry:
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 7
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
