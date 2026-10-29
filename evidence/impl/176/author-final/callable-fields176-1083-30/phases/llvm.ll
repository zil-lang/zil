; ZIL verified LLVM module
; verified-fingerprint: c26cd257539dfc55cc21631559f005b5caf22fe1ce7cdf880eeaa4c08f76de1c
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32 }
%zil_env_1 = type { i32, { i32 }, i32, i1 }
%zil_env_2 = type { %zil_env_1, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.1(ptr %env) {
entry:
  %flag.ptr.1 = getelementptr %zil_env_1, ptr %env, i32 0, i32 3
  %live.1 = load i1, ptr %flag.ptr.1, align 1
  br i1 %live.1, label %drop.1, label %done.1
drop.1:
  store i1 false, ptr %flag.ptr.1, align 1
  %payload.1 = getelementptr %zil_env_1, ptr %env, i32 0, i32 1
  call void @zil.fn.1(ptr %payload.1)
  br label %done.1
done.1:
  ret void
}
define internal void @zil.closure.drop.2(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_2, ptr %env, i32 0, i32 1
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %child.0 = getelementptr %zil_env_2, ptr %env, i32 0, i32 0
  call void @zil.closure.drop.1(ptr %child.0)
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
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  %v.2 = call i32 @putchar(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.2(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = insertvalue %zil_env_0 zeroinitializer, i32 %v.1, 0
  ret %zil_env_0 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_1 @zil.fn.3(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.2, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = load i32, ptr %local.0, align 4
  store i32 %v.3, ptr %local.3, align 4
  %v.4 = load i32, ptr %local.3, align 4
  %v.5 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.6 = load i32, ptr %local.0, align 4
  %tmp.0 = insertvalue %zil_env_1 zeroinitializer, i32 %v.4, 0
  %tmp.1 = insertvalue %zil_env_1 %tmp.0, { i32 } %v.5, 1
  %tmp.2 = insertvalue %zil_env_1 %tmp.1, i32 %v.6, 2
  %v.7 = insertvalue %zil_env_1 %tmp.2, i1 true, 3
  %tmp.3 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.3, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.2, align 1
  ret %zil_env_1 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_1 @zil.fn.4(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = call %zil_env_1 @zil.fn.3(i32 %v.0)
  ret %zil_env_1 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_2 @zil.fn.5(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca %zil_env_1, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = call %zil_env_1 @zil.fn.3(i32 %v.0)
  store i1 false, ptr %resource.live.1, align 1
  store %zil_env_1 %v.1, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.2 = load %zil_env_1, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %tmp.0 = insertvalue %zil_env_2 zeroinitializer, %zil_env_1 %v.2, 0
  %v.3 = insertvalue %zil_env_2 %tmp.0, i1 true, 1
  %tmp.1 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.1, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.closure.drop.1(ptr %local.1)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret %zil_env_2 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.6() {
entry:
  %enum.slot.0 = alloca { i32, [0 x i8] }, align 4
  br label %bb.0
bb.0:
  store { i32, [0 x i8] } zeroinitializer, ptr %enum.slot.0, align 4
  %tmp.0 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.0, i32 0, i32 0
  store i32 0, ptr %tmp.0, align 4
  %tmp.1 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.0, i32 0, i32 1
  %v.0 = load { i32, [0 x i8] }, ptr %enum.slot.0, align 4
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.1 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.2, { i32, [0 x i8] } %v.0, 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.7() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_1, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, %zil_env_1, i32 }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.0, align 1
  %v.0 = add i32 0, 65
  %v.1 = call %zil_env_1 @zil.fn.3(i32 %v.0)
  store %zil_env_1 %v.1, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  %v.2 = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.6()
  %v.3 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.2, 0
  br i1 %v.3, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.2, 0
  %tmp.1 = icmp ne i1 %tmp.0, true
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.4 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.2, 1
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.5 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.2, { i32, [0 x i8] } %v.4, 1
  %tmp.3 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.3, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.1(ptr %local.0)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.5
bb.2:
  %tmp.4 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.2, 0
  %tmp.5 = icmp ne i1 %tmp.4, false
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  %v.6 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.2, 2
  %v.7 = load %zil_env_1, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  %v.8 = call { { i32 }, %zil_env_1, i32 } @zil.fn.11(%zil_env_1 %v.7, i32 %v.6)
  %tmp.6 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.6, label %closure.drop.3, label %closure.done.3
closure.drop.3:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.1(ptr %local.0)
  br label %closure.done.3
closure.done.3:
  store i1 false, ptr %resource.live.1, align 1
  store { { i32 }, %zil_env_1, i32 } %v.8, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.9 = add i32 0, 0
  %tmp.7 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.10 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.7, i32 %v.9, 2
  %tmp.8 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.8, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.9 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.9)
  %tmp.10 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.10)
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.8(ptr %env) {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = add i32 0, 1
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.1, i32 %v.2)
  %v.3 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.3, ptr %local.0, align 4
  %tmp.3 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store i32 %v.3, ptr %tmp.3, align 4
  %v.4 = load i32, ptr %local.0, align 4
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.9(ptr %env) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca ptr, align 8
  %local.2 = alloca i32, align 4
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = getelementptr %zil_env_1, ptr %env, i32 0, i32 1
  store ptr %v.1, ptr %local.1, align 8
  %tmp.1 = getelementptr %zil_env_1, ptr %env, i32 0, i32 2
  %v.2 = load i32, ptr %tmp.1, align 4
  store i32 %v.2, ptr %local.2, align 4
  %v.3 = load i32, ptr %local.0, align 4
  %v.4 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.4)
  %v.5 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.5, ptr %local.0, align 4
  %tmp.4 = getelementptr %zil_env_1, ptr %env, i32 0, i32 0
  store i32 %v.5, ptr %tmp.4, align 4
  %v.6 = load i32, ptr %local.0, align 4
  %v.7 = load ptr, ptr %local.1, align 8
  %tmp.5 = getelementptr { i32 }, ptr %v.7, i32 0, i32 0
  %v.8 = load i32, ptr %tmp.5, align 4
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.6, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  %v.10 = load i32, ptr %local.2, align 4
  %tmp.8 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.10(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_2, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %v.2 = call i32 @zil.fn.9(ptr %v.1)
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, %zil_env_1, i32 } @zil.fn.11(%zil_env_1 %arg.0, i32 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_1, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca %zil_env_1, align 4
  %local.5 = alloca i32, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca %zil_env_1, align 4
  store %zil_env_1 %arg.0, ptr %local.0, align 4
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  %v.0 = load i32, ptr %local.1, align 4
  store i32 %v.0, ptr %local.2, align 4
  %v.1 = load i32, ptr %local.2, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.2, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.3 = load %zil_env_1, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.4, align 1
  store %zil_env_1 %v.3, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.4 = add i32 0, 0
  store i32 %v.4, ptr %local.5, align 4
  %v.5 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.5, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.6 = load %zil_env_1, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  store %zil_env_1 %v.6, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %v.7 = load i32, ptr %local.5, align 4
  %v.8 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  %v.9 = load %zil_env_1, ptr %local.7, align 4
  store i1 false, ptr %resource.live.7, align 1
%tmp.0 = insertvalue { { i32 }, %zil_env_1, i32 } zeroinitializer, { i32 } %v.8, 0
%tmp.1 = insertvalue { { i32 }, %zil_env_1, i32 } %tmp.0, %zil_env_1 %v.9, 1
%v.10 = insertvalue { { i32 }, %zil_env_1, i32 } %tmp.1, i32 %v.7, 2
  %tmp.2 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.2, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.closure.drop.1(ptr %local.7)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.3 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.1(ptr %local.6)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.4 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.4, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.1(ptr %local.4)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.5 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.5, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.1(ptr %local.3)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.6 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.6, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.1(ptr %local.0)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, %zil_env_1, i32 } %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.7()
  %entry.error = extractvalue { i1, { i32, [0 x i8] }, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, { i32, [0 x i8] }, i32 } %entry.result, 2
  ret i32 %entry.success
}
