; ZIL verified LLVM module
; verified-fingerprint: b99f8303ab5e90727eb3d0bf013cf9cfffa9573428f8726970648fe21d7a84b8
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { ptr, i64, i64 }, i1 }
%zil_env_1 = type { { ptr, i64, i64 }, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  call void @zil.bytes.drop(ptr %payload.0)
  br label %done.0
done.0:
  ret void
}
define internal void @zil.closure.drop.1(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 1
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 0
  call void @zil.bytes.drop(ptr %payload.0)
  br label %done.0
done.0:
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
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  %local.2 = alloca { ptr, i64 }, align 8
  br label %bb.0
bb.0:
  %v.0 = add i8 0, 7
  %v.1 = call %zil_env_0 @zil.fn.1(i8 %v.0)
  store i1 false, ptr %resource.live.0, align 1
  store %zil_env_0 %v.1, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.2 = call { ptr, i64 } @zil.fn.3(ptr %local.0)
  store { ptr, i64 } %v.2, ptr %local.1, align 8
  %v.3 = call { ptr, i64 } @zil.fn.3(ptr %local.0)
  store { ptr, i64 } %v.3, ptr %local.2, align 8
  %v.4 = load { ptr, i64 }, ptr %local.1, align 8
  %v.5 = extractvalue { ptr, i64 } %v.4, 1
  %v.6 = add i64 0, 1
  %v.7 = icmp ne i64 %v.5, %v.6
  br i1 %v.7, label %bb.1, label %bb.2
bb.1:
  %v.8 = add i32 0, 1
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.8
bb.2:
  br label %bb.3
bb.3:
  %v.9 = load { ptr, i64 }, ptr %local.2, align 8
  %v.10 = extractvalue { ptr, i64 } %v.9, 1
  %v.11 = add i64 0, 1
  %v.12 = icmp ne i64 %v.10, %v.11
  br i1 %v.12, label %bb.4, label %bb.5
bb.4:
  %v.13 = add i32 0, 2
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.13
bb.5:
  br label %bb.6
bb.6:
  %v.14 = load { ptr, i64 }, ptr %local.1, align 8
  %v.15 = add i32 0, 0
  %tmp.2 = extractvalue { ptr, i64 } %v.14, 0
  %tmp.3 = extractvalue { ptr, i64 } %v.14, 1
  %tmp.4 = sext i32 %v.15 to i64
  %tmp.5 = icmp uge i64 %tmp.4, %tmp.3
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  %v.16 = getelementptr i8, ptr %tmp.2, i64 %tmp.4
  %v.17 = getelementptr i8, ptr %v.16, i64 0
  %v.18 = load i8, ptr %v.17, align 1
  %v.19 = zext i8 %v.18 to i32
  %v.20 = load { ptr, i64 }, ptr %local.2, align 8
  %v.21 = add i32 0, 0
  %tmp.6 = extractvalue { ptr, i64 } %v.20, 0
  %tmp.7 = extractvalue { ptr, i64 } %v.20, 1
  %tmp.8 = sext i32 %v.21 to i64
  %tmp.9 = icmp uge i64 %tmp.8, %tmp.7
  br i1 %tmp.9, label %zil.trap, label %checked.3
checked.3:
  %v.22 = getelementptr i8, ptr %tmp.6, i64 %tmp.8
  %v.23 = getelementptr i8, ptr %v.22, i64 0
  %v.24 = load i8, ptr %v.23, align 1
  %v.25 = zext i8 %v.24 to i32
  %tmp.10 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.19, i32 %v.25)
  %v.26 = extractvalue { i32, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i32, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.4
checked.4:
  %v.27 = add i32 0, 14
  %tmp.12 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.26, i32 %v.27)
  %v.28 = extractvalue { i32, i1 } %tmp.12, 0
  %tmp.13 = extractvalue { i32, i1 } %tmp.12, 1
  br i1 %tmp.13, label %zil.trap, label %checked.5
checked.5:
  %tmp.14 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.14, label %closure.drop.6, label %closure.done.6
closure.drop.6:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.6
closure.done.6:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.28
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.1(i8 %arg.0) {
entry:
  %local.0 = alloca i8, align 1
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { ptr, i64, i64 }, align 8
  store i8 %arg.0, ptr %local.0, align 1
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = getelementptr i8, ptr %local.1, i64 0
  %v.2 = add i64 0, 4
  %v.3 = call i1 @zil.bytes.reserve(ptr %v.1, i64 %v.2)
  %v.4 = getelementptr i8, ptr %local.1, i64 0
  %v.5 = load i8, ptr %local.0, align 1
  %v.6 = call i1 @zil.bytes.push(ptr %v.4, i8 %v.5)
  %v.7 = load { ptr, i64, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, { ptr, i64, i64 } %v.7, 0
  %v.8 = insertvalue %zil_env_0 %tmp.0, i1 true, 1
  %tmp.1 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.bytes.drop(ptr %local.1)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret %zil_env_0 %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_1 @zil.fn.2() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { ptr, i64, i64 }, align 8
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = load { ptr, i64, i64 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  %tmp.0 = insertvalue %zil_env_1 zeroinitializer, { ptr, i64, i64 } %v.1, 0
  %v.2 = insertvalue %zil_env_1 %tmp.0, i1 true, 1
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.bytes.drop(ptr %local.0)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret %zil_env_1 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64 } @zil.fn.3(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %tmp.0 = load ptr, ptr %v.1, align 8
  %tmp.1 = getelementptr { ptr, i64, i64 }, ptr %v.1, i32 0, i32 1
  %tmp.2 = load i64, ptr %tmp.1, align 8
  %tmp.3 = insertvalue { ptr, i64 } poison, ptr %tmp.0, 0
  %v.2 = insertvalue { ptr, i64 } %tmp.3, i64 %tmp.2, 1
  ret { ptr, i64 } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64 } @zil.fn.4(ptr %env, i8 %arg.0) {
entry:
  %local.0 = alloca i8, align 1
  %local.1 = alloca ptr, align 8
  store i8 %arg.0, ptr %local.0, align 1
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.1, align 8
  %v.1 = load ptr, ptr %local.1, align 8
  %v.2 = load ptr, ptr %local.1, align 8
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.2, i32 0, i32 1
  %v.3 = load i64, ptr %tmp.0, align 8
  %v.4 = add i64 0, 1
  %tmp.1 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.3, i64 %v.4)
  %v.5 = extractvalue { i64, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i64, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %v.6 = call i1 @zil.bytes.reserve(ptr %v.1, i64 %v.5)
  %v.7 = load ptr, ptr %local.1, align 8
  %v.8 = load i8, ptr %local.0, align 1
  %v.9 = call i1 @zil.bytes.push(ptr %v.7, i8 %v.8)
  %v.10 = load ptr, ptr %local.1, align 8
  %tmp.3 = load ptr, ptr %v.10, align 8
  %tmp.4 = getelementptr { ptr, i64, i64 }, ptr %v.10, i32 0, i32 1
  %tmp.5 = load i64, ptr %tmp.4, align 8
  %tmp.6 = insertvalue { ptr, i64 } poison, ptr %tmp.3, 0
  %v.11 = insertvalue { ptr, i64 } %tmp.6, i64 %tmp.5, 1
  ret { ptr, i64 } %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
