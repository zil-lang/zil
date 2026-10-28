; ZIL verified LLVM module
; verified-fingerprint: e1faba6967d411deb2b71f055f5c44316fc583f2fb45a480b61ba8ccd452b415
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
  %local.0 = alloca %zil_env_1, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  %local.2 = alloca i32, align 4
  %local.3 = alloca { ptr, i64 }, align 8
  br label %bb.0
bb.0:
  %v.0 = call %zil_env_1 @zil.fn.2()
  store i1 false, ptr %resource.live.0, align 1
  store %zil_env_1 %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = add i8 0, 5
  %v.2 = call { ptr, i64 } @zil.fn.4(ptr %local.0, i8 %v.1)
  store { ptr, i64 } %v.2, ptr %local.1, align 8
  %v.3 = load { ptr, i64 }, ptr %local.1, align 8
  %v.4 = add i32 0, 0
  %tmp.0 = extractvalue { ptr, i64 } %v.3, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.3, 1
  %tmp.2 = sext i32 %v.4 to i64
  %tmp.3 = icmp uge i64 %tmp.2, %tmp.1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.5 = getelementptr i8, ptr %tmp.0, i64 %tmp.2
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = load i8, ptr %v.6, align 1
  %v.8 = zext i8 %v.7 to i32
  store i32 %v.8, ptr %local.2, align 4
  %v.9 = add i8 0, 8
  %v.10 = call { ptr, i64 } @zil.fn.4(ptr %local.0, i8 %v.9)
  store { ptr, i64 } %v.10, ptr %local.3, align 8
  %v.11 = load i32, ptr %local.2, align 4
  %v.12 = load { ptr, i64 }, ptr %local.3, align 8
  %v.13 = add i32 0, 0
  %tmp.4 = extractvalue { ptr, i64 } %v.12, 0
  %tmp.5 = extractvalue { ptr, i64 } %v.12, 1
  %tmp.6 = sext i32 %v.13 to i64
  %tmp.7 = icmp uge i64 %tmp.6, %tmp.5
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  %v.14 = getelementptr i8, ptr %tmp.4, i64 %tmp.6
  %v.15 = getelementptr i8, ptr %v.14, i64 0
  %v.16 = load i8, ptr %v.15, align 1
  %v.17 = zext i8 %v.16 to i32
  %tmp.8 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.11, i32 %v.17)
  %v.18 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  %v.19 = load { ptr, i64 }, ptr %local.3, align 8
  %v.20 = add i32 0, 1
  %tmp.10 = extractvalue { ptr, i64 } %v.19, 0
  %tmp.11 = extractvalue { ptr, i64 } %v.19, 1
  %tmp.12 = sext i32 %v.20 to i64
  %tmp.13 = icmp uge i64 %tmp.12, %tmp.11
  br i1 %tmp.13, label %zil.trap, label %checked.3
checked.3:
  %v.21 = getelementptr i8, ptr %tmp.10, i64 %tmp.12
  %v.22 = getelementptr i8, ptr %v.21, i64 0
  %v.23 = load i8, ptr %v.22, align 1
  %v.24 = zext i8 %v.23 to i32
  %tmp.14 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.18, i32 %v.24)
  %v.25 = extractvalue { i32, i1 } %tmp.14, 0
  %tmp.15 = extractvalue { i32, i1 } %tmp.14, 1
  br i1 %tmp.15, label %zil.trap, label %checked.4
checked.4:
  %v.26 = load { ptr, i64 }, ptr %local.3, align 8
  %v.27 = extractvalue { ptr, i64 } %v.26, 1
  %tmp.16 = icmp slt i64 %v.27, -2147483648
  %tmp.17 = icmp sgt i64 %v.27, 2147483647
  %tmp.18 = or i1 %tmp.16, %tmp.17
  br i1 %tmp.18, label %zil.trap, label %checked.5
checked.5:
  %v.28 = trunc i64 %v.27 to i32
  %tmp.19 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.25, i32 %v.28)
  %v.29 = extractvalue { i32, i1 } %tmp.19, 0
  %tmp.20 = extractvalue { i32, i1 } %tmp.19, 1
  br i1 %tmp.20, label %zil.trap, label %checked.6
checked.6:
  %v.30 = add i32 0, 20
  %tmp.21 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.29, i32 %v.30)
  %v.31 = extractvalue { i32, i1 } %tmp.21, 0
  %tmp.22 = extractvalue { i32, i1 } %tmp.21, 1
  br i1 %tmp.22, label %zil.trap, label %checked.7
checked.7:
  %tmp.23 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.23, label %closure.drop.8, label %closure.done.8
closure.drop.8:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.1(ptr %local.0)
  br label %closure.done.8
closure.done.8:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.31
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
