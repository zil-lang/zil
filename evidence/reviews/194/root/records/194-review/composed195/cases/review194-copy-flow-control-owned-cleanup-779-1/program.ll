; ZIL verified LLVM module
; verified-fingerprint: a90f98cef6edce9d51944c797c924d014e9f00035e143d9721d8ea6ef291a8e2
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { i32 }, i32, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  call void @zil.fn.1(ptr %payload.0)
  br label %done.0
done.0:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.1(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  %v.2 = call i32 @putchar(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca %zil_env_0, align 4
  %local.4 = alloca [2 x i64], align 8
  %local.5 = alloca [2 x i64], align 8
  %local.6 = alloca [2 x i64], align 8
  %index.slot.24 = alloca [2 x i64], align 8
  %index.slot.27 = alloca [2 x i64], align 8
  %index.slot.31 = alloca [2 x i64], align 8
  %index.slot.35 = alloca [2 x i64], align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 65
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = add i32 0, 0
  store i32 %v.3, ptr %local.2, align 4
  %v.4 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %v.5 = load i32, ptr %local.2, align 4
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, { i32 } %v.4, 0
  %tmp.1 = insertvalue %zil_env_0 %tmp.0, i32 %v.5, 1
  %v.6 = insertvalue %zil_env_0 %tmp.1, i1 true, 2
  store i1 false, ptr %resource.live.3, align 1
  store %zil_env_0 %v.6, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.7 = add i64 0, 0
  %v.8 = add i64 0, 1
%tmp.2 = insertvalue [2 x i64] zeroinitializer, i64 %v.7, 0
%v.9 = insertvalue [2 x i64] %tmp.2, i64 %v.8, 1
  store [2 x i64] %v.9, ptr %local.4, align 8
  %v.10 = getelementptr i8, ptr %local.3, i64 0
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = getelementptr i8, ptr %v.11, i64 0
  %v.13 = load [2 x i64], ptr %local.4, align 8
  %v.14 = add i32 0, 0
  %v.15 = call [2 x i64] @zil.fn.4(ptr %v.12, [2 x i64] %v.13, i32 %v.14)
  store [2 x i64] %v.15, ptr %local.5, align 8
  %v.16 = getelementptr i8, ptr %local.3, i64 0
  %v.17 = getelementptr i8, ptr %v.16, i64 0
  %v.18 = getelementptr i8, ptr %v.17, i64 0
  %v.19 = load [2 x i64], ptr %local.4, align 8
  %v.20 = add i32 0, 6
  %v.21 = call [2 x i64] @zil.fn.4(ptr %v.18, [2 x i64] %v.19, i32 %v.20)
  store [2 x i64] %v.21, ptr %local.6, align 8
  %v.22 = load [2 x i64], ptr %local.5, align 8
  %v.23 = add i32 0, 0
  %tmp.3 = sext i32 %v.23 to i64
  %tmp.4 = icmp ult i64 %tmp.3, 2
  br i1 %tmp.4, label %index.ok.0, label %zil.trap
index.ok.0:
  store [2 x i64] %v.22, ptr %index.slot.24, align 8
  %tmp.5 = getelementptr [2 x i64], ptr %index.slot.24, i64 0, i64 %tmp.3
  %v.24 = load i64, ptr %tmp.5, align 8
  %v.25 = load [2 x i64], ptr %local.5, align 8
  %v.26 = add i32 0, 1
  %tmp.6 = sext i32 %v.26 to i64
  %tmp.7 = icmp ult i64 %tmp.6, 2
  br i1 %tmp.7, label %index.ok.1, label %zil.trap
index.ok.1:
  store [2 x i64] %v.25, ptr %index.slot.27, align 8
  %tmp.8 = getelementptr [2 x i64], ptr %index.slot.27, i64 0, i64 %tmp.6
  %v.27 = load i64, ptr %tmp.8, align 8
  %tmp.9 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.24, i64 %v.27)
  %v.28 = extractvalue { i64, i1 } %tmp.9, 0
  %tmp.10 = extractvalue { i64, i1 } %tmp.9, 1
  br i1 %tmp.10, label %zil.trap, label %checked.2
checked.2:
  %v.29 = load [2 x i64], ptr %local.6, align 8
  %v.30 = add i32 0, 0
  %tmp.11 = sext i32 %v.30 to i64
  %tmp.12 = icmp ult i64 %tmp.11, 2
  br i1 %tmp.12, label %index.ok.3, label %zil.trap
index.ok.3:
  store [2 x i64] %v.29, ptr %index.slot.31, align 8
  %tmp.13 = getelementptr [2 x i64], ptr %index.slot.31, i64 0, i64 %tmp.11
  %v.31 = load i64, ptr %tmp.13, align 8
  %tmp.14 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.28, i64 %v.31)
  %v.32 = extractvalue { i64, i1 } %tmp.14, 0
  %tmp.15 = extractvalue { i64, i1 } %tmp.14, 1
  br i1 %tmp.15, label %zil.trap, label %checked.4
checked.4:
  %v.33 = load [2 x i64], ptr %local.6, align 8
  %v.34 = add i32 0, 1
  %tmp.16 = sext i32 %v.34 to i64
  %tmp.17 = icmp ult i64 %tmp.16, 2
  br i1 %tmp.17, label %index.ok.5, label %zil.trap
index.ok.5:
  store [2 x i64] %v.33, ptr %index.slot.35, align 8
  %tmp.18 = getelementptr [2 x i64], ptr %index.slot.35, i64 0, i64 %tmp.16
  %v.35 = load i64, ptr %tmp.18, align 8
  %tmp.19 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.32, i64 %v.35)
  %v.36 = extractvalue { i64, i1 } %tmp.19, 0
  %tmp.20 = extractvalue { i64, i1 } %tmp.19, 1
  br i1 %tmp.20, label %zil.trap, label %checked.6
checked.6:
  %tmp.21 = icmp slt i64 %v.36, -2147483648
  %tmp.22 = icmp sgt i64 %v.36, 2147483647
  %tmp.23 = or i1 %tmp.21, %tmp.22
  br i1 %tmp.23, label %zil.trap, label %checked.7
checked.7:
  %v.37 = trunc i64 %v.36 to i32
  %v.38 = add i32 0, 16
  %tmp.24 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.37, i32 %v.38)
  %v.39 = extractvalue { i32, i1 } %tmp.24, 0
  %tmp.25 = extractvalue { i32, i1 } %tmp.24, 1
  br i1 %tmp.25, label %zil.trap, label %checked.8
checked.8:
  %tmp.26 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.26, label %closure.drop.9, label %closure.done.9
closure.drop.9:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.closure.drop.0(ptr %local.3)
  br label %closure.done.9
closure.done.9:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.27 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.27, label %resource.drop.10, label %resource.done.10
resource.drop.10:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.10
resource.done.10:
  store i1 false, ptr %resource.live.1, align 1
  ret i32 %v.39
zil.trap:
  call void @llvm.trap()
  unreachable
}

define [2 x i64] @zil.fn.3(ptr %env, [2 x i64] %arg.0) {
entry:
  %local.0 = alloca [2 x i64], align 8
  %local.1 = alloca ptr, align 8
  %local.2 = alloca i32, align 4
  %index.slot.9 = alloca [2 x i64], align 8
  %index.slot.12 = alloca [2 x i64], align 8
  %index.slot.16 = alloca [2 x i64], align 8
  store [2 x i64] %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.1, align 8
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %v.1 = load i32, ptr %tmp.0, align 4
  store i32 %v.1, ptr %local.2, align 4
  %v.2 = load ptr, ptr %local.1, align 8
  %tmp.1 = getelementptr { i32 }, ptr %v.2, i32 0, i32 0
  %v.3 = load i32, ptr %tmp.1, align 4
  %v.4 = load i32, ptr %local.2, align 4
  %v.5 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.6, ptr %local.2, align 4
  %tmp.4 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store i32 %v.6, ptr %tmp.4, align 4
  %v.7 = load [2 x i64], ptr %local.0, align 8
  %v.8 = add i32 0, 0
  %tmp.5 = sext i32 %v.8 to i64
  %tmp.6 = icmp ult i64 %tmp.5, 2
  br i1 %tmp.6, label %index.ok.1, label %zil.trap
index.ok.1:
  store [2 x i64] %v.7, ptr %index.slot.9, align 8
  %tmp.7 = getelementptr [2 x i64], ptr %index.slot.9, i64 0, i64 %tmp.5
  %v.9 = load i64, ptr %tmp.7, align 8
  %v.10 = load [2 x i64], ptr %local.0, align 8
  %v.11 = add i32 0, 1
  %tmp.8 = sext i32 %v.11 to i64
  %tmp.9 = icmp ult i64 %tmp.8, 2
  br i1 %tmp.9, label %index.ok.2, label %zil.trap
index.ok.2:
  store [2 x i64] %v.10, ptr %index.slot.12, align 8
  %tmp.10 = getelementptr [2 x i64], ptr %index.slot.12, i64 0, i64 %tmp.8
  %v.12 = load i64, ptr %tmp.10, align 8
  %tmp.11 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.9, i64 %v.12)
  %v.13 = extractvalue { i64, i1 } %tmp.11, 0
  %tmp.12 = extractvalue { i64, i1 } %tmp.11, 1
  br i1 %tmp.12, label %zil.trap, label %checked.3
checked.3:
  %v.14 = load [2 x i64], ptr %local.0, align 8
  %v.15 = add i32 0, 1
  %tmp.13 = sext i32 %v.15 to i64
  %tmp.14 = icmp ult i64 %tmp.13, 2
  br i1 %tmp.14, label %index.ok.4, label %zil.trap
index.ok.4:
  store [2 x i64] %v.14, ptr %index.slot.16, align 8
  %tmp.15 = getelementptr [2 x i64], ptr %index.slot.16, i64 0, i64 %tmp.13
  %v.16 = load i64, ptr %tmp.15, align 8
  %v.17 = add i64 0, 1
  %tmp.16 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.16, i64 %v.17)
  %v.18 = extractvalue { i64, i1 } %tmp.16, 0
  %tmp.17 = extractvalue { i64, i1 } %tmp.16, 1
  br i1 %tmp.17, label %zil.trap, label %checked.5
checked.5:
%tmp.18 = insertvalue [2 x i64] zeroinitializer, i64 %v.13, 0
%v.19 = insertvalue [2 x i64] %tmp.18, i64 %v.18, 1
  ret [2 x i64] %v.19
zil.trap:
  call void @llvm.trap()
  unreachable
}

define [2 x i64] @zil.fn.4(ptr %arg.0, [2 x i64] %arg.1, i32 %arg.2) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca [2 x i64], align 8
  %local.2 = alloca i32, align 4
  %local.3 = alloca [2 x i64], align 8
  %local.4 = alloca i32, align 4
  %local.5 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store [2 x i64] %arg.1, ptr %local.1, align 8
  store i32 %arg.2, ptr %local.2, align 4
  br label %bb.0
bb.0:
  %v.0 = load [2 x i64], ptr %local.1, align 8
  store [2 x i64] %v.0, ptr %local.3, align 8
  %v.1 = add i32 0, 0
  %v.2 = load i32, ptr %local.2, align 4
  store i32 %v.1, ptr %local.4, align 4
  br label %bb.1
bb.1:
  %v.3 = load i32, ptr %local.4, align 4
  %v.4 = icmp slt i32 %v.3, %v.2
  br i1 %v.4, label %bb.2, label %bb.4
bb.2:
  store i32 %v.3, ptr %local.5, align 4
  %v.5 = load i32, ptr %local.5, align 4
  %v.6 = add i32 0, 1
  %v.7 = icmp eq i32 %v.5, %v.6
  br i1 %v.7, label %bb.5, label %bb.6
bb.3:
  %v.14 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.14)
  %v.15 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.15, ptr %local.4, align 4
  br label %bb.1
bb.4:
  %v.16 = load [2 x i64], ptr %local.3, align 8
  ret [2 x i64] %v.16
bb.5:
  br label %bb.3
bb.6:
  br label %bb.7
bb.7:
  %v.8 = load i32, ptr %local.5, align 4
  %v.9 = add i32 0, 5
  %v.10 = icmp sge i32 %v.8, %v.9
  br i1 %v.10, label %bb.8, label %bb.9
bb.8:
  br label %bb.4
bb.9:
  br label %bb.10
bb.10:
  %v.11 = load ptr, ptr %local.0, align 8
  %v.12 = load [2 x i64], ptr %local.3, align 8
  %v.13 = call [2 x i64] @zil.fn.3(ptr %v.11, [2 x i64] %v.12)
  store [2 x i64] %v.13, ptr %local.3, align 8
  br label %bb.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.2()
  ret i32 %entry.result
}
