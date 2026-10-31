; ZIL verified LLVM module
; verified-fingerprint: 6e293c7518e2d035ff5de916dc3a6c8c2a41f7408761da794d6c5ff5133b4750
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)
define i32 @zil.fn.0() {
entry:
  %local.0 = alloca { i32 }, align 4
  %local.1 = alloca { i32 }, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 10
  %v.1 = call { i32 } @zil.fn.7(i32 %v.0)
  store { i32 } %v.1, ptr %local.0, align 4
  %v.2 = getelementptr i8, ptr %local.0, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = add i32 0, 20
  call void @zil.fn.3(ptr %v.3, i32 %v.4)
  %v.6 = add i32 0, 2
  %v.7 = call { i32 } @zil.fn.7(i32 %v.6)
  store { i32 } %v.7, ptr %local.1, align 4
  %v.8 = getelementptr i8, ptr %local.0, i64 0
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = call i32 @zil.fn.1(ptr %v.10)
  %v.12 = add i32 0, 20
  %v.13 = icmp ne i32 %v.11, %v.12
  br i1 %v.13, label %bb.1, label %bb.2
bb.1:
  %v.14 = add i32 0, 1
  ret i32 %v.14
bb.2:
  br label %bb.3
bb.3:
  %v.15 = getelementptr i8, ptr %local.0, i64 0
  %v.16 = getelementptr i8, ptr %v.15, i64 0
  %v.17 = getelementptr i8, ptr %local.1, i64 0
  %v.18 = getelementptr i8, ptr %v.17, i64 0
  %v.19 = getelementptr i8, ptr %v.18, i64 0
  %v.20 = call i32 @zil.fn.4(ptr %v.16, ptr %v.19)
  %v.21 = add i32 0, 22
  %v.22 = icmp ne i32 %v.20, %v.21
  br i1 %v.22, label %bb.4, label %bb.5
bb.4:
  %v.23 = add i32 0, 2
  ret i32 %v.23
bb.5:
  br label %bb.6
bb.6:
  %v.24 = load { i32 }, ptr %local.1, align 4
  %v.25 = call i32 @zil.fn.6({ i32 } %v.24)
  %v.26 = add i32 0, 2
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.25, i32 %v.26)
  %v.27 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.27
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
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = call i32 @zil.fn.2(ptr %v.1)
  ret i32 %v.2
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
  %v.1 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load i32, ptr %v.2, align 4
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}
define void @zil.fn.3(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.2 = load i32, ptr %local.1, align 4
  store i32 %v.2, ptr %v.1, align 4
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.4(ptr %arg.0, ptr %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store ptr %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load i32, ptr %v.2, align 4
  %v.4 = load ptr, ptr %local.1, align 8
  %v.5 = getelementptr { i32 }, ptr %v.4, i32 0, i32 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = load i32, ptr %v.6, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}
define { i32 } @zil.fn.5(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  ret { i32 } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.6({ i32 } %arg.0) {
entry:
  %local.0 = alloca { i32 }, align 4
  store { i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { i32 }, ptr %local.0, align 4
  %v.1 = extractvalue { i32 } %v.0, 0
  ret i32 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}
define { i32 } @zil.fn.7(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = call { i32 } @zil.fn.5(i32 %v.0)
  ret { i32 } %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
