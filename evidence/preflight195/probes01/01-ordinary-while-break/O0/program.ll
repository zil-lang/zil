; ZIL verified LLVM module
; verified-fingerprint: e79be763e3e627157f077908fb621a23f4c454cd517303d5c63d3eb0a1f77582
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  br label %bb.1
bb.1:
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = add i32 0, 5
  %v.3 = icmp slt i32 %v.1, %v.2
  br i1 %v.3, label %bb.2, label %bb.3
bb.2:
  %v.4 = load i32, ptr %local.0, align 4
  %v.5 = add i32 0, 3
  %v.6 = icmp eq i32 %v.4, %v.5
  br i1 %v.6, label %bb.4, label %bb.5
bb.3:
  %v.10 = load i32, ptr %local.0, align 4
  %v.11 = add i32 0, 3
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.10, i32 %v.11)
  %v.12 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.12
bb.4:
  br label %bb.3
bb.5:
  br label %bb.6
bb.6:
  %v.7 = load i32, ptr %local.0, align 4
  %v.8 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.7, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.9, ptr %local.0, align 4
  br label %bb.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
