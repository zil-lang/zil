; ZIL verified LLVM module
; verified-fingerprint: c564f94879e0837a87a2951cbd3653c5f875797d208af76b55760f5e4fc5ef45
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 0
  store i32 %v.1, ptr %local.1, align 4
  br label %bb.1
bb.1:
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = add i32 0, 5
  %v.4 = icmp slt i32 %v.2, %v.3
  br i1 %v.4, label %bb.2, label %bb.3
bb.2:
  %v.5 = load i32, ptr %local.0, align 4
  %v.6 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.7, ptr %local.0, align 4
  %v.8 = load i32, ptr %local.0, align 4
  %v.9 = add i32 0, 2
  %v.10 = icmp eq i32 %v.8, %v.9
  br i1 %v.10, label %bb.4, label %bb.5
bb.3:
  %v.14 = load i32, ptr %local.1, align 4
  %v.15 = add i32 0, 13
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.14, i32 %v.15)
  %v.16 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.16
bb.4:
  br label %bb.1
bb.5:
  br label %bb.6
bb.6:
  %v.11 = load i32, ptr %local.1, align 4
  %v.12 = load i32, ptr %local.0, align 4
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.11, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  store i32 %v.13, ptr %local.1, align 4
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
