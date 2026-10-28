; ZIL verified LLVM module
; verified-fingerprint: 1dad69d737719b03e775c5add1ff91027fe7beee9db719832aa59c8882848f3b
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define { { i32 }, i32, i32 } @zil.fn.0() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i32 }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.4, align 1
  %v.0 = add i32 0, 1
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = add i32 0, 20
  store i32 %v.3, ptr %local.2, align 4
  %v.4 = add i32 0, 1
  store i32 %v.4, ptr %local.3, align 4
  %v.5 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.5, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.6 = load i32, ptr %local.2, align 4
  %v.7 = load i32, ptr %local.3, align 4
  %v.8 = load { i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
%tmp.0 = insertvalue { { i32 }, i32, i32 } zeroinitializer, { i32 } %v.8, 0
%tmp.1 = insertvalue { { i32 }, i32, i32 } %tmp.0, i32 %v.6, 1
%v.9 = insertvalue { { i32 }, i32, i32 } %tmp.1, i32 %v.7, 2
  %tmp.2 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.2, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.4, align 1
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.3 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  ret { { i32 }, i32, i32 } %v.9
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1(i1 %arg.0) {
entry:
  %local.0 = alloca i1, align 1
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, i32, i32 }, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %local.4 = alloca ptr, align 8
  store i1 %arg.0, ptr %local.0, align 1
  br label %bb.0
bb.0:
  %v.0 = call { { i32 }, i32, i32 } @zil.fn.0()
  store i1 false, ptr %resource.live.1, align 1
  store { { i32 }, i32, i32 } %v.0, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = add i32 0, 0
  store i32 %v.1, ptr %local.2, align 4
  %v.2 = add i32 0, 0
  store i32 %v.2, ptr %local.3, align 4
  br label %bb.1
bb.1:
  %v.3 = load i32, ptr %local.3, align 4
  %v.4 = add i32 0, 3
  %v.5 = icmp slt i32 %v.3, %v.4
  br i1 %v.5, label %bb.2, label %bb.3
bb.2:
  %v.6 = getelementptr { { i32 }, i32, i32 }, ptr %local.1, i32 0, i32 1
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  store ptr %v.7, ptr %local.4, align 8
  %v.8 = load i1, ptr %local.0, align 1
  br i1 %v.8, label %bb.4, label %bb.5
bb.3:
  %v.26 = load i32, ptr %local.2, align 4
  %tmp.0 = getelementptr { { i32 }, i32, i32 }, ptr %local.1, i32 0, i32 1
  %v.27 = load i32, ptr %tmp.0, align 4
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.26, i32 %v.27)
  %v.28 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %tmp.3 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.3, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.1, align 1
  ret i32 %v.28
bb.4:
  %v.9 = load ptr, ptr %local.4, align 8
  %v.10 = load ptr, ptr %local.4, align 8
  %v.11 = load i32, ptr %v.10, align 4
  %v.12 = add i32 0, 1
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.11, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  store i32 %v.13, ptr %v.9, align 4
  br label %bb.6
bb.5:
  %v.14 = load ptr, ptr %local.4, align 8
  %v.15 = load ptr, ptr %local.4, align 8
  %v.16 = load i32, ptr %v.15, align 4
  %v.17 = add i32 0, 2
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.16, i32 %v.17)
  %v.18 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
  store i32 %v.18, ptr %v.14, align 4
  br label %bb.6
bb.6:
  %v.19 = load i32, ptr %local.2, align 4
  %v.20 = load ptr, ptr %local.4, align 8
  %v.21 = load i32, ptr %v.20, align 4
  %tmp.8 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.19, i32 %v.21)
  %v.22 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.4
checked.4:
  store i32 %v.22, ptr %local.2, align 4
  %v.23 = load i32, ptr %local.3, align 4
  %v.24 = add i32 0, 1
  %tmp.10 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.23, i32 %v.24)
  %v.25 = extractvalue { i32, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i32, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.5
checked.5:
  store i32 %v.25, ptr %local.3, align 4
  br label %bb.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2() {
entry:
  br label %bb.0
bb.0:
  %v.0 = or i1 false, true
  %v.1 = call i32 @zil.fn.1(i1 %v.0)
  %v.2 = or i1 false, false
  %v.3 = call i32 @zil.fn.1(i1 %v.2)
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.1, i32 %v.3)
  %v.4 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.5 = add i32 0, 187
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.2()
  ret i32 %entry.result
}
