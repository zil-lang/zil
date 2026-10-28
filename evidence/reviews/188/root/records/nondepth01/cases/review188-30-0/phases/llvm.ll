; ZIL verified LLVM module
; verified-fingerprint: 5b1cda1faf864d17c2a1dde3af12fa4a3d3ed5070a4e38963cd0360da8e87a52
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { ptr, i64, i64 }, i1 }
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
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca %zil_env_0, align 8
  %local.2 = alloca { ptr, i64 }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca %zil_env_0, align 8
  br label %bb.0
bb.0:
  %v.0 = add i8 0, 65
  %v.1 = call %zil_env_0 @zil.fn.1(i8 %v.0)
  store i1 false, ptr %resource.live.0, align 1
  store %zil_env_0 %v.1, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.2 = add i8 0, 66
  %v.3 = call %zil_env_0 @zil.fn.1(i8 %v.2)
  store i1 false, ptr %resource.live.1, align 1
  store %zil_env_0 %v.3, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.4 = getelementptr i8, ptr %local.0, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = call { ptr, i64 } @zil.fn.3(ptr %v.6)
  store { ptr, i64 } %v.7, ptr %local.2, align 8
  %v.8 = load %zil_env_0, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.3, align 1
  store %zil_env_0 %v.8, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.9 = load { ptr, i64 }, ptr %local.2, align 8
  %v.10 = add i32 0, 0
  %tmp.0 = extractvalue { ptr, i64 } %v.9, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.9, 1
  %tmp.2 = sext i32 %v.10 to i64
  %tmp.3 = icmp uge i64 %tmp.2, %tmp.1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.11 = getelementptr i8, ptr %tmp.0, i64 %tmp.2
  %v.12 = getelementptr i8, ptr %v.11, i64 0
  %v.13 = load i8, ptr %v.12, align 1
  %v.14 = zext i8 %v.13 to i32
  %v.15 = call { ptr, i64 } @zil.fn.2(ptr %local.3)
  %v.16 = add i32 0, 0
  %tmp.4 = extractvalue { ptr, i64 } %v.15, 0
  %tmp.5 = extractvalue { ptr, i64 } %v.15, 1
  %tmp.6 = sext i32 %v.16 to i64
  %tmp.7 = icmp uge i64 %tmp.6, %tmp.5
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  %v.17 = getelementptr i8, ptr %tmp.4, i64 %tmp.6
  %v.18 = getelementptr i8, ptr %v.17, i64 0
  %v.19 = load i8, ptr %v.18, align 1
  %v.20 = zext i8 %v.19 to i32
  %tmp.8 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.14, i32 %v.20)
  %v.21 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  %v.22 = add i32 0, 131
  %tmp.10 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.21, i32 %v.22)
  %v.23 = extractvalue { i32, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i32, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.3
checked.3:
  %tmp.12 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.12, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.closure.drop.0(ptr %local.3)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.13 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.13, label %closure.drop.5, label %closure.done.5
closure.drop.5:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.closure.drop.0(ptr %local.1)
  br label %closure.done.5
closure.done.5:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.14 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.14, label %closure.drop.6, label %closure.done.6
closure.drop.6:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.6
closure.done.6:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.23
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
  %v.2 = add i64 0, 1
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

define { ptr, i64 } @zil.fn.2(ptr %env) {
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

define { ptr, i64 } @zil.fn.3(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = call { ptr, i64 } @zil.fn.4(ptr %v.1)
  store { ptr, i64 } %v.2, ptr %local.1, align 8
  %v.3 = load ptr, ptr %local.0, align 8
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = call { ptr, i64 } @zil.fn.4(ptr %v.4)
  ret { ptr, i64 } %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64 } @zil.fn.4(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = call { ptr, i64 } @zil.fn.2(ptr %v.0)
  ret { ptr, i64 } %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
