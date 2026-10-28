; ZIL verified LLVM module
; verified-fingerprint: 1edbe5286c3f349de5f1d0f5f5afcda769795974eb5555af1b1095252786aa2e
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { i32 }, { ptr, i64, i64 }, i1, i1 }
%zil_env_1 = type { { i32 }, { ptr, i64, i64 }, i1, i1 }
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
  %flag.ptr.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 3
  %live.1 = load i1, ptr %flag.ptr.1, align 1
  br i1 %live.1, label %drop.1, label %done.1
drop.1:
  store i1 false, ptr %flag.ptr.1, align 1
  %payload.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  call void @zil.bytes.drop(ptr %payload.1)
  br label %done.1
done.1:
  ret void
}
define internal void @zil.closure.drop.1(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 2
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 0
  call void @zil.fn.1(ptr %payload.0)
  br label %done.0
done.0:
  %flag.ptr.1 = getelementptr %zil_env_1, ptr %env, i32 0, i32 3
  %live.1 = load i1, ptr %flag.ptr.1, align 1
  br i1 %live.1, label %drop.1, label %done.1
drop.1:
  store i1 false, ptr %flag.ptr.1, align 1
  %payload.1 = getelementptr %zil_env_1, ptr %env, i32 0, i32 1
  call void @zil.bytes.drop(ptr %payload.1)
  br label %done.1
done.1:
  ret void
}
declare void @llvm.trap()
declare ptr @realloc(ptr, i64)
declare void @free(ptr)

define internal ptr @zil.bytes.reallocate(ptr %old, i64 %size) {
entry:
  %result = call ptr @realloc(ptr %old, i64 %size)
  ret ptr %result
}
define internal void @zil.bytes.release(ptr %payload) {
entry:
  call void @free(ptr %payload)
  ret void
}

define internal i1 @zil.bytes.reserve(ptr %head, i64 %requested) {
entry:
  %negative = icmp slt i64 %requested, 0
  br i1 %negative, label %failed, label %capacity
capacity:
  %cap.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 2
  %cap = load i64, ptr %cap.field, align 8
  %fits = icmp ule i64 %requested, %cap
  br i1 %fits, label %done, label %prepare
prepare:
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %old = load ptr, ptr %ptr.field, align 8
  %next = call ptr @zil.bytes.reallocate(ptr %old, i64 %requested)
  %ok = icmp ne ptr %next, null
  br i1 %ok, label %publish, label %failed
publish:
  store ptr %next, ptr %ptr.field, align 8
  store i64 %requested, ptr %cap.field, align 8
  br label %done
done:
  ret i1 true
failed:
  ret i1 false
}
define internal i1 @zil.bytes.truncate(ptr %head, i64 %requested) {
entry:
  %len.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 1
  %len = load i64, ptr %len.field, align 8
  %nonnegative = icmp sge i64 %requested, 0
  %within = icmp ule i64 %requested, %len
  %valid = and i1 %nonnegative, %within
  br i1 %valid, label %publish, label %failed
publish:
  store i64 %requested, ptr %len.field, align 8
  ret i1 true
failed:
  ret i1 false
}
define internal i1 @zil.bytes.push(ptr %head, i8 %byte) {
entry:
  %len.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 1
  %cap.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 2
  %len = load i64, ptr %len.field, align 8
  %cap = load i64, ptr %cap.field, align 8
  %room = icmp ult i64 %len, %cap
  br i1 %room, label %initialize, label %full
initialize:
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %payload = load ptr, ptr %ptr.field, align 8
  %slot = getelementptr i8, ptr %payload, i64 %len
  store i8 %byte, ptr %slot, align 1
  %next.len = add i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  ret i1 true
full:
  ret i1 false
}
define internal void @zil.bytes.drop(ptr %head) {
entry:
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %payload = load ptr, ptr %ptr.field, align 8
  call void @zil.bytes.release(ptr %payload)
  ret void
}
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)
declare { i64, i1 } @llvm.ssub.with.overflow.i64(i64, i64)

define i32 @zil.fn.0() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_1, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { ptr, i64, i64 }, %zil_env_1, i64 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { { ptr, i64, i64 }, %zil_env_1, i64 }, align 8
  %local.5 = alloca i32, align 4
  %local.6 = alloca { ptr, i64 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = add i8 0, 65
  %v.1 = call %zil_env_0 @zil.fn.2(i8 %v.0)
  store i1 false, ptr %resource.live.0, align 1
  store %zil_env_0 %v.1, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.2 = getelementptr i8, ptr %local.0, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = call { ptr, i64 } @zil.fn.9(ptr %v.4)
  store { ptr, i64 } %v.5, ptr %local.1, align 8
  %v.6 = load { ptr, i64 }, ptr %local.1, align 8
  %v.7 = add i32 0, 0
  %tmp.0 = extractvalue { ptr, i64 } %v.6, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.6, 1
  %tmp.2 = sext i32 %v.7 to i64
  %tmp.3 = icmp uge i64 %tmp.2, %tmp.1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.8 = getelementptr i8, ptr %tmp.0, i64 %tmp.2
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = load i8, ptr %v.9, align 1
  %v.11 = zext i8 %v.10 to i32
  call void @zil.fn.5(i32 %v.11)
  %v.13 = call %zil_env_1 @zil.fn.3()
  store %zil_env_1 %v.13, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.14 = load %zil_env_1, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  %v.15 = call { { ptr, i64, i64 }, %zil_env_1, i64 } @zil.fn.10(%zil_env_1 %v.14)
  %tmp.4 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.4, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.1(ptr %local.2)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.3, align 1
  store { { ptr, i64, i64 }, %zil_env_1, i64 } %v.15, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.16 = load { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  store { { ptr, i64, i64 }, %zil_env_1, i64 } %v.16, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.17 = add i64 0, 3
  %v.18 = load { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  %v.19 = call i32 @zil.fn.13({ { ptr, i64, i64 }, %zil_env_1, i64 } %v.18, i64 %v.17)
  store i32 %v.19, ptr %local.5, align 4
  %tmp.5 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.5, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.6 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.4, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.6)
  %tmp.7 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.4, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.7)
  br label %package.done.2
package.done.2:
  %v.20 = load i32, ptr %local.5, align 4
  %v.21 = add i32 0, 0
  %v.22 = icmp ne i32 %v.20, %v.21
  br i1 %v.22, label %bb.1, label %bb.2
bb.1:
  %v.23 = load i32, ptr %local.5, align 4
  store i1 false, ptr %resource.live.4, align 1
  %tmp.8 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.8, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.9 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.3, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.9)
  %tmp.10 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.10)
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.11 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.11, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.23
bb.2:
  br label %bb.3
bb.3:
  %v.24 = getelementptr i8, ptr %local.0, i64 0
  %v.25 = getelementptr i8, ptr %v.24, i64 0
  %v.26 = getelementptr i8, ptr %v.25, i64 0
  %v.27 = call { ptr, i64 } @zil.fn.9(ptr %v.26)
  store { ptr, i64 } %v.27, ptr %local.6, align 8
  %v.28 = load { ptr, i64 }, ptr %local.6, align 8
  %v.29 = add i32 0, 0
  %tmp.12 = extractvalue { ptr, i64 } %v.28, 0
  %tmp.13 = extractvalue { ptr, i64 } %v.28, 1
  %tmp.14 = sext i32 %v.29 to i64
  %tmp.15 = icmp uge i64 %tmp.14, %tmp.13
  br i1 %tmp.15, label %zil.trap, label %checked.5
checked.5:
  %v.30 = getelementptr i8, ptr %tmp.12, i64 %tmp.14
  %v.31 = getelementptr i8, ptr %v.30, i64 0
  %v.32 = load i8, ptr %v.31, align 1
  %v.33 = zext i8 %v.32 to i32
  call void @zil.fn.5(i32 %v.33)
  %v.35 = load { ptr, i64 }, ptr %local.1, align 8
  %v.36 = add i32 0, 0
  %tmp.16 = extractvalue { ptr, i64 } %v.35, 0
  %tmp.17 = extractvalue { ptr, i64 } %v.35, 1
  %tmp.18 = sext i32 %v.36 to i64
  %tmp.19 = icmp uge i64 %tmp.18, %tmp.17
  br i1 %tmp.19, label %zil.trap, label %checked.6
checked.6:
  %v.37 = getelementptr i8, ptr %tmp.16, i64 %tmp.18
  %v.38 = getelementptr i8, ptr %v.37, i64 0
  %v.39 = load i8, ptr %v.38, align 1
  %v.40 = zext i8 %v.39 to i32
  %v.41 = load { ptr, i64 }, ptr %local.6, align 8
  %v.42 = add i32 0, 0
  %tmp.20 = extractvalue { ptr, i64 } %v.41, 0
  %tmp.21 = extractvalue { ptr, i64 } %v.41, 1
  %tmp.22 = sext i32 %v.42 to i64
  %tmp.23 = icmp uge i64 %tmp.22, %tmp.21
  br i1 %tmp.23, label %zil.trap, label %checked.7
checked.7:
  %v.43 = getelementptr i8, ptr %tmp.20, i64 %tmp.22
  %v.44 = getelementptr i8, ptr %v.43, i64 0
  %v.45 = load i8, ptr %v.44, align 1
  %v.46 = zext i8 %v.45 to i32
  %tmp.24 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.40, i32 %v.46)
  %v.47 = extractvalue { i32, i1 } %tmp.24, 0
  %tmp.25 = extractvalue { i32, i1 } %tmp.24, 1
  br i1 %tmp.25, label %zil.trap, label %checked.8
checked.8:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.26 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.26, label %package.drop.9, label %package.done.9
package.drop.9:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.27 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.3, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.27)
  %tmp.28 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.28)
  br label %package.done.9
package.done.9:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.29 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.29, label %closure.drop.10, label %closure.done.10
closure.drop.10:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.10
closure.done.10:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.47
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.1(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  call void @zil.fn.5(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.2(i8 %arg.0) {
entry:
  %local.0 = alloca i8, align 1
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { ptr, i64, i64 }, align 8
  store i8 %arg.0, ptr %local.0, align 1
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 90
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.2, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.3, align 1
  store { ptr, i64, i64 } %v.3, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.4 = getelementptr i8, ptr %local.3, i64 0
  %v.5 = add i64 0, 1
  %v.6 = call i1 @zil.bytes.reserve(ptr %v.4, i64 %v.5)
  %v.7 = getelementptr i8, ptr %local.3, i64 0
  %v.8 = load i8, ptr %local.0, align 1
  %v.9 = call i1 @zil.bytes.push(ptr %v.7, i8 %v.8)
  %v.10 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.11 = load { ptr, i64, i64 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, { i32 } %v.10, 0
  %tmp.1 = insertvalue %zil_env_0 %tmp.0, { ptr, i64, i64 } %v.11, 1
  %tmp.2 = insertvalue %zil_env_0 %tmp.1, i1 true, 2
  %v.12 = insertvalue %zil_env_0 %tmp.2, i1 true, 3
  %tmp.3 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.3, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.bytes.drop(ptr %local.3)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.4 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.4, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  ret %zil_env_0 %v.12
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_1 @zil.fn.3() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { ptr, i64, i64 }, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 72
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.2, align 1
  store { ptr, i64, i64 } %v.3, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.4 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %v.5 = load { ptr, i64, i64 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  %tmp.0 = insertvalue %zil_env_1 zeroinitializer, { i32 } %v.4, 0
  %tmp.1 = insertvalue %zil_env_1 %tmp.0, { ptr, i64, i64 } %v.5, 1
  %tmp.2 = insertvalue %zil_env_1 %tmp.1, i1 true, 2
  %v.6 = insertvalue %zil_env_1 %tmp.2, i1 true, 3
  %tmp.3 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.3, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.bytes.drop(ptr %local.2)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.4 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.4, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  ret %zil_env_1 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.5(i32 %arg.0) {
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

define { ptr, i64 } @zil.fn.6(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store ptr %v.1, ptr %local.1, align 8
  %v.2 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.2, i32 0, i32 0
  %v.3 = load i32, ptr %tmp.0, align 4
  %v.4 = load ptr, ptr %local.1, align 8
  %tmp.1 = load ptr, ptr %v.4, align 8
  %tmp.2 = getelementptr { ptr, i64, i64 }, ptr %v.4, i32 0, i32 1
  %tmp.3 = load i64, ptr %tmp.2, align 8
  %tmp.4 = insertvalue { ptr, i64 } poison, ptr %tmp.1, 0
  %v.5 = insertvalue { ptr, i64 } %tmp.4, i64 %tmp.3, 1
  ret { ptr, i64 } %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64 } @zil.fn.7(ptr %env, i8 %arg.0) {
entry:
  %local.0 = alloca i8, align 1
  %local.1 = alloca ptr, align 8
  %local.2 = alloca ptr, align 8
  store i8 %arg.0, ptr %local.0, align 1
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.1, align 8
  %v.1 = getelementptr %zil_env_1, ptr %env, i32 0, i32 1
  store ptr %v.1, ptr %local.2, align 8
  %v.2 = load ptr, ptr %local.1, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.2, i32 0, i32 0
  %v.3 = load i32, ptr %tmp.0, align 4
  %v.4 = load ptr, ptr %local.2, align 8
  %v.5 = load ptr, ptr %local.2, align 8
  %tmp.1 = getelementptr { ptr, i64, i64 }, ptr %v.5, i32 0, i32 1
  %v.6 = load i64, ptr %tmp.1, align 8
  %v.7 = add i64 0, 1
  %tmp.2 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.6, i64 %v.7)
  %v.8 = extractvalue { i64, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i64, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.9 = call i1 @zil.bytes.reserve(ptr %v.4, i64 %v.8)
  %v.10 = load ptr, ptr %local.2, align 8
  %v.11 = load i8, ptr %local.0, align 1
  %v.12 = call i1 @zil.bytes.push(ptr %v.10, i8 %v.11)
  %v.13 = load ptr, ptr %local.2, align 8
  %tmp.4 = load ptr, ptr %v.13, align 8
  %tmp.5 = getelementptr { ptr, i64, i64 }, ptr %v.13, i32 0, i32 1
  %tmp.6 = load i64, ptr %tmp.5, align 8
  %tmp.7 = insertvalue { ptr, i64 } poison, ptr %tmp.4, 0
  %v.14 = insertvalue { ptr, i64 } %tmp.7, i64 %tmp.6, 1
  ret { ptr, i64 } %v.14
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64 } @zil.fn.8(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = call { ptr, i64 } @zil.fn.6(ptr %v.0)
  ret { ptr, i64 } %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64 } @zil.fn.9(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = call { ptr, i64 } @zil.fn.8(ptr %v.1)
  ret { ptr, i64 } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { ptr, i64, i64 }, %zil_env_1, i64 } @zil.fn.10(%zil_env_1 %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_1, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { ptr, i64, i64 }, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_1, align 8
  %local.3 = alloca i64, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { ptr, i64, i64 }, align 8
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca %zil_env_1, align 8
  store %zil_env_1 %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = load %zil_env_1, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_1 %v.1, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.2 = add i64 0, 0
  store i64 %v.2, ptr %local.3, align 8
  %v.3 = load { ptr, i64, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.3, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.4 = load %zil_env_1, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_1 %v.4, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.5 = load i64, ptr %local.3, align 8
  %v.6 = load { ptr, i64, i64 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  %v.7 = load %zil_env_1, ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
%tmp.0 = insertvalue { { ptr, i64, i64 }, %zil_env_1, i64 } zeroinitializer, { ptr, i64, i64 } %v.6, 0
%tmp.1 = insertvalue { { ptr, i64, i64 }, %zil_env_1, i64 } %tmp.0, %zil_env_1 %v.7, 1
%v.8 = insertvalue { { ptr, i64, i64 }, %zil_env_1, i64 } %tmp.1, i64 %v.5, 2
  %tmp.2 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.2, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.closure.drop.1(ptr %local.5)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.3 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.bytes.drop(ptr %local.4)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.4 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.4, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.1(ptr %local.2)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.5 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.5, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.bytes.drop(ptr %local.1)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.6 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.6, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.1(ptr %local.0)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret { { ptr, i64, i64 }, %zil_env_1, i64 } %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64 } @zil.fn.11(ptr %arg.0, i8 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i8, align 1
  store ptr %arg.0, ptr %local.0, align 8
  store i8 %arg.1, ptr %local.1, align 1
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i8, ptr %local.1, align 1
  %v.2 = call { ptr, i64 } @zil.fn.7(ptr %v.0, i8 %v.1)
  ret { ptr, i64 } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64 } @zil.fn.12(ptr %arg.0, i8 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i8, align 1
  store ptr %arg.0, ptr %local.0, align 8
  store i8 %arg.1, ptr %local.1, align 1
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = load i8, ptr %local.1, align 1
  %v.3 = call { ptr, i64 } @zil.fn.11(ptr %v.1, i8 %v.2)
  ret { ptr, i64 } %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.13({ { ptr, i64, i64 }, %zil_env_1, i64 } %arg.0, i64 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { ptr, i64, i64 }, %zil_env_1, i64 }, align 8
  %local.1 = alloca i64, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { ptr, i64, i64 }, %zil_env_1, i64 }, align 8
  %package.live.2.0 = alloca i1, align 1
  store i1 false, ptr %package.live.2.0, align 1
  %package.live.2.1 = alloca i1, align 1
  store i1 false, ptr %package.live.2.1, align 1
  %local.3 = alloca ptr, align 8
  %local.4 = alloca { ptr, i64 }, align 8
  store { { ptr, i64, i64 }, %zil_env_1, i64 } %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  store { { ptr, i64, i64 }, %zil_env_1, i64 } %v.0, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  store i1 true, ptr %package.live.2.0, align 1
  store i1 true, ptr %package.live.2.1, align 1
  br label %bb.1
bb.1:
  %tmp.0 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.2, i32 0, i32 2
  %v.1 = load i64, ptr %tmp.0, align 8
  %v.2 = load i64, ptr %local.1, align 8
  %v.3 = icmp slt i64 %v.1, %v.2
  br i1 %v.3, label %bb.2, label %bb.3
bb.2:
  %v.4 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.2, i32 0, i32 1
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  store ptr %v.5, ptr %local.3, align 8
  %v.6 = load ptr, ptr %local.3, align 8
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %tmp.1 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.2, i32 0, i32 2
  %v.8 = load i64, ptr %tmp.1, align 8
  %v.9 = add i64 0, 66
  %tmp.2 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.8, i64 %v.9)
  %v.10 = extractvalue { i64, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i64, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %tmp.4 = icmp ugt i64 %v.10, 255
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.11 = trunc i64 %v.10 to i8
  %v.12 = call { ptr, i64 } @zil.fn.12(ptr %v.7, i8 %v.11)
  store { ptr, i64 } %v.12, ptr %local.4, align 8
  %tmp.5 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.2, i32 0, i32 2
  %v.13 = load i64, ptr %tmp.5, align 8
  %v.14 = add i64 0, 1
  %tmp.6 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.13, i64 %v.14)
  %v.15 = extractvalue { i64, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i64, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.2
checked.2:
  %tmp.8 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.2, i32 0, i32 2
  store i64 %v.15, ptr %tmp.8, align 8
  %v.16 = load { ptr, i64 }, ptr %local.4, align 8
  %v.17 = extractvalue { ptr, i64 } %v.16, 1
  %tmp.9 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.2, i32 0, i32 2
  %v.18 = load i64, ptr %tmp.9, align 8
  %v.19 = icmp ne i64 %v.17, %v.18
  br i1 %v.19, label %bb.4, label %bb.5
bb.3:
  %v.30 = add i32 0, 0
  %tmp.10 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.10, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.11 = load i1, ptr %package.live.2.0, align 1
  br i1 %tmp.11, label %package.field.drop.4, label %package.field.done.4
package.field.drop.4:
  store i1 false, ptr %package.live.2.0, align 1
  %tmp.12 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.2, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.12)
  br label %package.field.done.4
package.field.done.4:
  %tmp.13 = load i1, ptr %package.live.2.1, align 1
  br i1 %tmp.13, label %package.field.drop.5, label %package.field.done.5
package.field.drop.5:
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.14 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.14)
  br label %package.field.done.5
package.field.done.5:
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.15 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.15, label %package.drop.6, label %package.done.6
package.drop.6:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.16 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.0, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.16)
  %tmp.17 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.17)
  br label %package.done.6
package.done.6:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.30
bb.4:
  %v.20 = add i32 0, 1
  %tmp.18 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.18, label %package.drop.7, label %package.done.7
package.drop.7:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.19 = load i1, ptr %package.live.2.0, align 1
  br i1 %tmp.19, label %package.field.drop.8, label %package.field.done.8
package.field.drop.8:
  store i1 false, ptr %package.live.2.0, align 1
  %tmp.20 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.2, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.20)
  br label %package.field.done.8
package.field.done.8:
  %tmp.21 = load i1, ptr %package.live.2.1, align 1
  br i1 %tmp.21, label %package.field.drop.9, label %package.field.done.9
package.field.drop.9:
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.22 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.22)
  br label %package.field.done.9
package.field.done.9:
  br label %package.done.7
package.done.7:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.23 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.23, label %package.drop.10, label %package.done.10
package.drop.10:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.24 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.0, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.24)
  %tmp.25 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.25)
  br label %package.done.10
package.done.10:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.20
bb.5:
  br label %bb.6
bb.6:
  %v.21 = load { ptr, i64 }, ptr %local.4, align 8
  %tmp.26 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.2, i32 0, i32 2
  %v.22 = load i64, ptr %tmp.26, align 8
  %v.23 = add i64 0, 1
  %tmp.27 = call { i64, i1 } @llvm.ssub.with.overflow.i64(i64 %v.22, i64 %v.23)
  %v.24 = extractvalue { i64, i1 } %tmp.27, 0
  %tmp.28 = extractvalue { i64, i1 } %tmp.27, 1
  br i1 %tmp.28, label %zil.trap, label %checked.11
checked.11:
  %tmp.29 = extractvalue { ptr, i64 } %v.21, 0
  %tmp.30 = extractvalue { ptr, i64 } %v.21, 1
  %tmp.31 = icmp uge i64 %v.24, %tmp.30
  br i1 %tmp.31, label %zil.trap, label %checked.12
checked.12:
  %v.25 = getelementptr i8, ptr %tmp.29, i64 %v.24
  %v.26 = getelementptr i8, ptr %v.25, i64 0
  %v.27 = load i8, ptr %v.26, align 1
  %v.28 = zext i8 %v.27 to i32
  call void @zil.fn.5(i32 %v.28)
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
