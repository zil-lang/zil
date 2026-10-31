; ZIL verified LLVM module
; verified-fingerprint: ec7939ca44e1cec648578c9870f690336174ac7857e1fe649819c5a8bb1e58f9
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
  %v.0 = add i32 0, 17
  %v.1 = call { i32 } @zil.fn.2(i32 %v.0)
  store { i32 } %v.1, ptr %local.0, align 4
  %v.2 = add i32 0, 24
  %v.3 = call { i32 } @zil.fn.4(i32 %v.2)
  store { i32 } %v.3, ptr %local.1, align 4
  %v.4 = getelementptr i8, ptr %local.0, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = call i32 @zil.fn.5(ptr %v.6)
  %v.8 = getelementptr i8, ptr %local.1, i64 0
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = call i32 @zil.fn.6(ptr %v.10)
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.7, i32 %v.11)
  %v.12 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.13 = add i32 0, 42
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.12, i32 %v.13)
  %v.14 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.14
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
  %v.1 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load i32, ptr %v.2, align 4
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}
define { i32 } @zil.fn.2(i32 %arg.0) {
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
define i32 @zil.fn.3(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load i32, ptr %v.2, align 4
  %v.4 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.4)
  %v.5 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}
define { i32 } @zil.fn.4(i32 %arg.0) {
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
define i32 @zil.fn.5(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = call i32 @zil.fn.1(ptr %v.1)
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.6(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = call i32 @zil.fn.3(ptr %v.1)
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
