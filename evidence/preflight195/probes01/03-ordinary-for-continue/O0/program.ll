; ZIL verified LLVM module
; verified-fingerprint: fcfcc1d437683043132705f7b709574ff0502aa83454afc0bbfbc8e41ffce62f
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 0
  %v.2 = add i32 0, 5
  store i32 %v.1, ptr %local.1, align 4
  br label %bb.1
bb.1:
  %v.3 = load i32, ptr %local.1, align 4
  %v.4 = icmp slt i32 %v.3, %v.2
  br i1 %v.4, label %bb.2, label %bb.4
bb.2:
  store i32 %v.3, ptr %local.2, align 4
  %v.5 = load i32, ptr %local.2, align 4
  %v.6 = add i32 0, 2
  %v.7 = icmp eq i32 %v.5, %v.6
  br i1 %v.7, label %bb.5, label %bb.6
bb.3:
  %v.11 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.11)
  %v.12 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.12, ptr %local.1, align 4
  br label %bb.1
bb.4:
  %v.13 = load i32, ptr %local.0, align 4
  %v.14 = add i32 0, 8
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.13, i32 %v.14)
  %v.15 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.15
bb.5:
  br label %bb.3
bb.6:
  br label %bb.7
bb.7:
  %v.8 = load i32, ptr %local.0, align 4
  %v.9 = load i32, ptr %local.2, align 4
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.8, i32 %v.9)
  %v.10 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  store i32 %v.10, ptr %local.0, align 4
  br label %bb.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
