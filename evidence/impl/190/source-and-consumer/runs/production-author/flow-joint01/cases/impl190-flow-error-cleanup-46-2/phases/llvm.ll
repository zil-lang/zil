; ZIL verified LLVM module
; verified-fingerprint: fd2df839231769e21af80847472a139590246e93a0520a35d0a9619a8891465f
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
  call void @zil.fn.4(ptr %payload.0)
  br label %done.0
done.0:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @getchar() nonlazybind
define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = call i32 @getchar()
  store i32 %v.1, ptr %local.0, align 4
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = add i32 0, 48
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.2, i32 %v.3)
  %v.4 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.3(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = call i32 @putchar(i32 %v.0)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.4(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  call void @zil.fn.3(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.5() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca %zil_env_0, align 4
  br label %bb.0
bb.0:
  %v.0 = call i32 @zil.fn.1()
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 65
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = load i32, ptr %local.1, align 4
%v.3 = insertvalue { i32 } zeroinitializer, i32 %v.2, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.3, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.4 = add i32 0, 0
  store i32 %v.4, ptr %local.3, align 4
  %v.5 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.6 = load i32, ptr %local.3, align 4
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, { i32 } %v.5, 0
  %tmp.1 = insertvalue %zil_env_0 %tmp.0, i32 %v.6, 1
  %v.7 = insertvalue %zil_env_0 %tmp.1, i1 true, 2
  store i1 false, ptr %resource.live.4, align 1
  store %zil_env_0 %v.7, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.8 = getelementptr i8, ptr %local.4, i64 0
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = load i32, ptr %local.0, align 4
  %v.12 = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.7(ptr %v.10, i32 %v.11)
  %v.13 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.12, 0
  br i1 %v.13, label %bb.1, label %bb.2
bb.1:
  %tmp.2 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.12, 0
  %tmp.3 = icmp ne i1 %tmp.2, true
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.14 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.12, 1
  %tmp.4 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.15 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.4, { i32, [0 x i8] } %v.14, 1
  %tmp.5 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.5, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.0(ptr %local.4)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.6 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.6, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.4(ptr %local.2)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.15
bb.2:
  %tmp.7 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.12, 0
  %tmp.8 = icmp ne i1 %tmp.7, false
  br i1 %tmp.8, label %zil.trap, label %checked.3
checked.3:
  %v.16 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.12, 2
  %v.17 = load i32, ptr %local.0, align 4
  %tmp.9 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.16, i32 %v.17)
  %v.18 = extractvalue { i32, i1 } %tmp.9, 0
  %tmp.10 = extractvalue { i32, i1 } %tmp.9, 1
  br i1 %tmp.10, label %zil.trap, label %checked.4
checked.4:
  %tmp.11 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.19 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.11, i32 %v.18, 2
  %tmp.12 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.12, label %closure.drop.5, label %closure.done.5
closure.drop.5:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.0(ptr %local.4)
  br label %closure.done.5
closure.done.5:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.13 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.13, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.4(ptr %local.2)
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.19
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.6(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %enum.slot.10 = alloca { i32, [0 x i8] }, align 4
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %v.1 = load i32, ptr %tmp.0, align 4
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = load ptr, ptr %local.0, align 8
  %tmp.1 = getelementptr { i32 }, ptr %v.2, i32 0, i32 0
  %v.3 = load i32, ptr %tmp.1, align 4
  %v.4 = load i32, ptr %local.1, align 4
  %v.5 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.6, ptr %local.1, align 4
  %tmp.4 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store i32 %v.6, ptr %tmp.4, align 4
  %v.7 = load i32, ptr %local.1, align 4
  %v.8 = add i32 0, 2
  %v.9 = icmp eq i32 %v.7, %v.8
  br i1 %v.9, label %bb.1, label %bb.2
bb.1:
  store { i32, [0 x i8] } zeroinitializer, ptr %enum.slot.10, align 4
  %tmp.5 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.10, i32 0, i32 0
  store i32 0, ptr %tmp.5, align 4
  %tmp.6 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.10, i32 0, i32 1
  %v.10 = load { i32, [0 x i8] }, ptr %enum.slot.10, align 4
  %tmp.7 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.11 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.7, { i32, [0 x i8] } %v.10, 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.11
bb.2:
  br label %bb.3
bb.3:
  %v.12 = add i32 0, 1
  %tmp.8 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.13 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.8, i32 %v.12, 2
  ret { i1, { i32, [0 x i8] }, i32 } %v.13
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.7(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %local.4 = alloca i32, align 4
  %local.5 = alloca i32, align 4
  %local.6 = alloca i32, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { i32 }, align 4
  %local.8 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 67
  store i32 %v.0, ptr %local.2, align 4
  %v.1 = load i32, ptr %local.2, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.2, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.3 = add i32 0, 0
  store i32 %v.3, ptr %local.4, align 4
  %v.4 = add i32 0, 0
  store i32 %v.4, ptr %local.5, align 4
  br label %bb.1
bb.1:
  %v.5 = load i32, ptr %local.4, align 4
  %v.6 = load i32, ptr %local.1, align 4
  %v.7 = icmp slt i32 %v.5, %v.6
  br i1 %v.7, label %bb.2, label %bb.3
bb.2:
  %v.8 = add i32 0, 66
  store i32 %v.8, ptr %local.6, align 4
  %v.9 = load i32, ptr %local.6, align 4
%v.10 = insertvalue { i32 } zeroinitializer, i32 %v.9, 0
  store i1 false, ptr %resource.live.7, align 1
  store { i32 } %v.10, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %v.11 = load ptr, ptr %local.0, align 8
  %v.12 = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.6(ptr %v.11)
  %v.13 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.12, 0
  br i1 %v.13, label %bb.4, label %bb.5
bb.3:
  %v.23 = load i32, ptr %local.5, align 4
  %tmp.0 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.24 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.0, i32 %v.23, 2
  %tmp.1 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.4(ptr %local.3)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.24
bb.4:
  %tmp.2 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.12, 0
  %tmp.3 = icmp ne i1 %tmp.2, true
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  %v.14 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.12, 1
  %tmp.4 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.15 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.4, { i32, [0 x i8] } %v.14, 1
  %tmp.5 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.5, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.fn.4(ptr %local.7)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.6 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.6, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.4(ptr %local.3)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.15
bb.5:
  %tmp.7 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.12, 0
  %tmp.8 = icmp ne i1 %tmp.7, false
  br i1 %tmp.8, label %zil.trap, label %checked.4
checked.4:
  %v.16 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.12, 2
  store i32 %v.16, ptr %local.8, align 4
  %v.17 = load i32, ptr %local.5, align 4
  %v.18 = load i32, ptr %local.8, align 4
  %tmp.9 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.17, i32 %v.18)
  %v.19 = extractvalue { i32, i1 } %tmp.9, 0
  %tmp.10 = extractvalue { i32, i1 } %tmp.9, 1
  br i1 %tmp.10, label %zil.trap, label %checked.5
checked.5:
  store i32 %v.19, ptr %local.5, align 4
  %v.20 = load i32, ptr %local.4, align 4
  %v.21 = add i32 0, 1
  %tmp.11 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.20, i32 %v.21)
  %v.22 = extractvalue { i32, i1 } %tmp.11, 0
  %tmp.12 = extractvalue { i32, i1 } %tmp.11, 1
  br i1 %tmp.12, label %zil.trap, label %checked.6
checked.6:
  store i32 %v.22, ptr %local.4, align 4
  %tmp.13 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.13, label %resource.drop.7, label %resource.done.7
resource.drop.7:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.fn.4(ptr %local.7)
  br label %resource.done.7
resource.done.7:
  store i1 false, ptr %resource.live.7, align 1
  br label %bb.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.5()
  %entry.error = extractvalue { i1, { i32, [0 x i8] }, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, { i32, [0 x i8] }, i32 } %entry.result, 2
  ret i32 %entry.success
}
