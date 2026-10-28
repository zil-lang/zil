; ZIL verified LLVM module
; verified-fingerprint: 5d350b694f126d988178587c47ee06f29361229c61d500ce5bb4c3ed42ac41b7
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define ptr @zil.fn.0(ptr %arg.0, ptr %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store ptr %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.1, align 8
  %v.1 = load ptr, ptr %local.1, align 8
  %v.2 = load i32, ptr %v.1, align 4
  %v.3 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.2, i32 %v.3)
  %v.4 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.4, ptr %v.0, align 4
  %v.5 = load ptr, ptr %local.0, align 8
  ret ptr %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 7
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store { i32 } %v.2, ptr %local.1, align 4
  %v.3 = add i32 0, 0
  store i32 %v.3, ptr %local.2, align 4
  %v.4 = getelementptr i8, ptr %local.1, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = getelementptr i8, ptr %local.2, i64 0
  %v.8 = getelementptr i8, ptr %v.7, i64 0
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = call i32 @zil.fn.2(ptr %v.6, ptr %v.9)
  store i32 %v.10, ptr %local.3, align 4
  %v.11 = load i32, ptr %local.3, align 4
  %v.12 = load i32, ptr %local.2, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.11, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.14 = add i32 0, 16
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.13, i32 %v.14)
  %v.15 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.15
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(ptr %arg.0, ptr %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store ptr %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = load ptr, ptr %local.1, align 8
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = call ptr @zil.fn.0(ptr %v.1, ptr %v.3)
  %v.5 = getelementptr { i32 }, ptr %v.4, i32 0, i32 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = load i32, ptr %v.6, align 4
  %v.8 = load ptr, ptr %local.0, align 8
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = load ptr, ptr %local.1, align 8
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = call ptr @zil.fn.0(ptr %v.9, ptr %v.11)
  %v.13 = getelementptr { i32 }, ptr %v.12, i32 0, i32 0
  %v.14 = getelementptr i8, ptr %v.13, i64 0
  %v.15 = load i32, ptr %v.14, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.7, i32 %v.15)
  %v.16 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.16
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}
