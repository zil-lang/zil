; ZIL verified LLVM module
; verified-fingerprint: e144e1ef6c08b0a56cefe4b85a4a63765e13cbfe90f6ae8b2c08eb54457e79e5
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

define i32 @zil.fn.1(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i32, ptr %v.0, align 4
  ret i32 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %v.2 = load i32, ptr %v.1, align 4
  %v.3 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.2, i32 %v.3)
  %v.4 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.4, ptr %v.0, align 4
  %v.5 = load ptr, ptr %local.0, align 8
  %v.6 = load i32, ptr %v.5, align 4
  ret i32 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, i32, i32 }, align 4
  %package.live.0.0 = alloca i1, align 1
  store i1 false, ptr %package.live.0.0, align 1
  %local.1 = alloca ptr, align 8
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %local.4 = alloca i32, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { { i32 }, i32, i32 }, align 4
  br label %bb.0
bb.0:
  %v.0 = call { { i32 }, i32, i32 } @zil.fn.0()
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %package.live.0.0, align 1
  store { { i32 }, i32, i32 } %v.0, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  store i1 true, ptr %package.live.0.0, align 1
  %v.1 = getelementptr { { i32 }, i32, i32 }, ptr %local.0, i32 0, i32 1
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  store ptr %v.2, ptr %local.1, align 8
  %v.3 = load ptr, ptr %local.1, align 8
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = call i32 @zil.fn.1(ptr %v.4)
  store i32 %v.5, ptr %local.2, align 4
  %v.6 = load ptr, ptr %local.1, align 8
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = call i32 @zil.fn.2(ptr %v.7)
  store i32 %v.8, ptr %local.3, align 4
  %v.9 = load ptr, ptr %local.1, align 8
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = call i32 @zil.fn.2(ptr %v.10)
  store i32 %v.11, ptr %local.4, align 4
  %tmp.0 = getelementptr { { i32 }, i32, i32 }, ptr %local.0, i32 0, i32 2
  %v.12 = load i32, ptr %tmp.0, align 4
  %v.13 = load i32, ptr %local.2, align 4
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.12, i32 %v.13)
  %v.14 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %tmp.3 = getelementptr { { i32 }, i32, i32 }, ptr %local.0, i32 0, i32 2
  store i32 %v.14, ptr %tmp.3, align 4
  %v.15 = load { { i32 }, i32, i32 }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %package.live.0.0, align 1
  store i1 false, ptr %resource.live.5, align 1
  store { { i32 }, i32, i32 } %v.15, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %tmp.4 = getelementptr { { i32 }, i32, i32 }, ptr %local.5, i32 0, i32 1
  %v.16 = load i32, ptr %tmp.4, align 4
  %tmp.5 = getelementptr { { i32 }, i32, i32 }, ptr %local.5, i32 0, i32 2
  %v.17 = load i32, ptr %tmp.5, align 4
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.16, i32 %v.17)
  %v.18 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  %v.19 = load i32, ptr %local.2, align 4
  %tmp.8 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.18, i32 %v.19)
  %v.20 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  %v.21 = load i32, ptr %local.3, align 4
  %tmp.10 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.20, i32 %v.21)
  %v.22 = extractvalue { i32, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i32, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.3
checked.3:
  %v.23 = load i32, ptr %local.4, align 4
  %tmp.12 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.22, i32 %v.23)
  %v.24 = extractvalue { i32, i1 } %tmp.12, 0
  %tmp.13 = extractvalue { i32, i1 } %tmp.12, 1
  br i1 %tmp.13, label %zil.trap, label %checked.4
checked.4:
  %v.25 = add i32 0, 106
  %tmp.14 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.24, i32 %v.25)
  %v.26 = extractvalue { i32, i1 } %tmp.14, 0
  %tmp.15 = extractvalue { i32, i1 } %tmp.14, 1
  br i1 %tmp.15, label %zil.trap, label %checked.5
checked.5:
  %tmp.16 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.16, label %package.drop.6, label %package.done.6
package.drop.6:
  store i1 false, ptr %resource.live.5, align 1
  br label %package.done.6
package.done.6:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.17 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.17, label %package.drop.7, label %package.done.7
package.drop.7:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.18 = load i1, ptr %package.live.0.0, align 1
  br i1 %tmp.18, label %package.field.drop.8, label %package.field.done.8
package.field.drop.8:
  store i1 false, ptr %package.live.0.0, align 1
  br label %package.field.done.8
package.field.done.8:
  br label %package.done.7
package.done.7:
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %package.live.0.0, align 1
  ret i32 %v.26
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.3()
  ret i32 %entry.result
}
