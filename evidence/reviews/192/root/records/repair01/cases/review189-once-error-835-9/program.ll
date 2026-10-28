; ZIL verified LLVM module
; verified-fingerprint: 0d2d8b31bdcdc13097beb599847671c9259f580db20295835fbecfed1ab59bf3
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { i32, i32 }, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
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
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.1(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32, i32 }, ptr %v.0, i32 0, i32 1
  %v.1 = load i32, ptr %tmp.0, align 4
  %v.2 = call i32 @putchar(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.2({ i32, i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { i32, i32 }, align 4
  store { i32, i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.fn.1(ptr %local.0)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.3() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32, i32 }, align 4
  %local.3 = alloca i32, align 4
  %local.4 = alloca i32, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i32, i32 }, align 4
  %local.6 = alloca i32, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca %zil_env_0, align 4
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca %zil_env_0, align 4
  %local.9 = alloca i32, align 4
  %closure.once.18 = alloca %zil_env_0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.8, align 1
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 90
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = load i32, ptr %local.1, align 4
%tmp.0 = insertvalue { i32, i32 } zeroinitializer, i32 %v.2, 0
%v.4 = insertvalue { i32, i32 } %tmp.0, i32 %v.3, 1
  store i1 false, ptr %resource.live.2, align 1
  store { i32, i32 } %v.4, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.5 = add i32 0, 2
  store i32 %v.5, ptr %local.3, align 4
  %v.6 = add i32 0, 65
  store i32 %v.6, ptr %local.4, align 4
  %v.7 = load i32, ptr %local.3, align 4
  %v.8 = load i32, ptr %local.4, align 4
%tmp.1 = insertvalue { i32, i32 } zeroinitializer, i32 %v.7, 0
%v.9 = insertvalue { i32, i32 } %tmp.1, i32 %v.8, 1
  store i1 false, ptr %resource.live.5, align 1
  store { i32, i32 } %v.9, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.10 = add i32 0, 40
  store i32 %v.10, ptr %local.6, align 4
  %v.11 = load { i32, i32 }, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
  %tmp.2 = insertvalue %zil_env_0 zeroinitializer, { i32, i32 } %v.11, 0
  %v.12 = insertvalue %zil_env_0 %tmp.2, i1 true, 1
  store i1 false, ptr %resource.live.7, align 1
  store %zil_env_0 %v.12, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %v.13 = load %zil_env_0, ptr %local.7, align 4
  store i1 false, ptr %resource.live.7, align 1
  store %zil_env_0 %v.13, ptr %local.8, align 4
  store i1 true, ptr %resource.live.8, align 1
  %v.14 = getelementptr i8, ptr %local.6, i64 0
  %v.15 = getelementptr i8, ptr %v.14, i64 0
  %v.16 = getelementptr i8, ptr %v.15, i64 0
  %v.17 = or i1 false, true
  %v.18 = load %zil_env_0, ptr %local.8, align 4
  store i1 false, ptr %resource.live.8, align 1
  store %zil_env_0 %v.18, ptr %closure.once.18, align 4
  %v.19 = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.4(ptr %closure.once.18, ptr %v.16, i1 %v.17)
  call void @zil.closure.drop.0(ptr %closure.once.18)
  %v.20 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.19, 0
  br i1 %v.20, label %bb.1, label %bb.2
bb.1:
  %tmp.3 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.19, 0
  %tmp.4 = icmp ne i1 %tmp.3, true
  br i1 %tmp.4, label %zil.trap, label %checked.0
checked.0:
  %v.21 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.19, 1
  %tmp.5 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.22 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.5, { i32, [0 x i8] } %v.21, 1
  %tmp.6 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.6, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.closure.drop.0(ptr %local.8)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.7 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.7, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.closure.drop.0(ptr %local.7)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.8 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.8, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.1(ptr %local.5)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.9 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.9, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.22
bb.2:
  %tmp.10 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.19, 0
  %tmp.11 = icmp ne i1 %tmp.10, false
  br i1 %tmp.11, label %zil.trap, label %checked.5
checked.5:
  %v.23 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.19, 2
  store i32 %v.23, ptr %local.9, align 4
  %tmp.12 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.12, label %closure.drop.6, label %closure.done.6
closure.drop.6:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.closure.drop.0(ptr %local.8)
  br label %closure.done.6
closure.done.6:
  %v.24 = load i32, ptr %local.9, align 4
  %v.25 = load i32, ptr %local.6, align 4
  %tmp.13 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.24, i32 %v.25)
  %v.26 = extractvalue { i32, i1 } %tmp.13, 0
  %tmp.14 = extractvalue { i32, i1 } %tmp.13, 1
  br i1 %tmp.14, label %zil.trap, label %checked.7
checked.7:
  %v.27 = add i32 0, 84
  %tmp.15 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.26, i32 %v.27)
  %v.28 = extractvalue { i32, i1 } %tmp.15, 0
  %tmp.16 = extractvalue { i32, i1 } %tmp.15, 1
  br i1 %tmp.16, label %zil.trap, label %checked.8
checked.8:
  %tmp.17 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.29 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.17, i32 %v.28, 2
  store i1 false, ptr %resource.live.8, align 1
  %tmp.18 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.18, label %closure.drop.9, label %closure.done.9
closure.drop.9:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.closure.drop.0(ptr %local.7)
  br label %closure.done.9
closure.done.9:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.19 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.19, label %resource.drop.10, label %resource.done.10
resource.drop.10:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.1(ptr %local.5)
  br label %resource.done.10
resource.done.10:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.20 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.20, label %resource.drop.11, label %resource.done.11
resource.drop.11:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.11
resource.done.11:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.29
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.4(ptr %env, ptr %arg.0, i1 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i1, align 1
  %local.2 = alloca ptr, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32, i32 }, align 4
  %enum.slot.11 = alloca { i32, [0 x i8] }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i1 %arg.1, ptr %local.1, align 1
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.2, align 8
  store i1 false, ptr %resource.live.3, align 1
  %v.1 = load ptr, ptr %local.0, align 8
  %v.2 = load ptr, ptr %local.0, align 8
  %v.3 = load i32, ptr %v.2, align 4
  %v.4 = load ptr, ptr %local.2, align 8
  %tmp.0 = getelementptr { i32, i32 }, ptr %v.4, i32 0, i32 0
  %v.5 = load i32, ptr %tmp.0, align 4
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.6, ptr %v.1, align 4
  %tmp.3 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store i1 false, ptr %tmp.3, align 1
  %tmp.4 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.7 = load { i32, i32 }, ptr %tmp.4, align 4
  store { i32, i32 } %v.7, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.8 = load { i32, i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.2({ i32, i32 } %v.8)
  %tmp.5 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.5, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.1(ptr %local.3)
  br label %resource.done.1
resource.done.1:
  %v.10 = load i1, ptr %local.1, align 1
  br i1 %v.10, label %bb.1, label %bb.2
bb.1:
  store { i32, [0 x i8] } zeroinitializer, ptr %enum.slot.11, align 4
  %tmp.6 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.11, i32 0, i32 0
  store i32 0, ptr %tmp.6, align 4
  %tmp.7 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.11, i32 0, i32 1
  %v.11 = load { i32, [0 x i8] }, ptr %enum.slot.11, align 4
  %tmp.8 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.12 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.8, { i32, [0 x i8] } %v.11, 1
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.12
bb.2:
  br label %bb.3
bb.3:
  %v.13 = load ptr, ptr %local.0, align 8
  %v.14 = load i32, ptr %v.13, align 4
  %tmp.9 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.15 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.9, i32 %v.14, 2
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.15
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.3()
  %entry.error = extractvalue { i1, { i32, [0 x i8] }, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, { i32, [0 x i8] }, i32 } %entry.result, 2
  ret i32 %entry.success
}
