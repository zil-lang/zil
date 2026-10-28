; ZIL verified LLVM module
; verified-fingerprint: d79995fcbfad0cd02844550ae9741cbcee5ad0b78ffd846ac08aaec78332e50b
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
declare { i8, i1 } @llvm.uadd.with.overflow.i8(i8, i8)

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

define %zil_env_0 @zil.fn.2() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca i32, align 4
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
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret %zil_env_0 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { %zil_env_0, { ptr, i64, i64 }, i32 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %v.0 = call %zil_env_0 @zil.fn.2()
  store %zil_env_0 %v.0, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = load %zil_env_0, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  %v.2 = call { %zil_env_0, { ptr, i64, i64 }, i32 } @zil.fn.5(%zil_env_0 %v.1)
  store { %zil_env_0, { ptr, i64, i64 }, i32 } %v.2, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = load { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  %v.4 = call i32 @zil.fn.6({ %zil_env_0, { ptr, i64, i64 }, i32 } %v.3)
  %v.5 = add i32 0, 24
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.3 = getelementptr { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.closure.drop.0(ptr %tmp.3)
  %tmp.4 = getelementptr { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.4)
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.5 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.5, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.4(ptr %env, { ptr, i64 } %arg.0, ptr %arg.1) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  %local.1 = alloca ptr, align 8
  %local.2 = alloca ptr, align 8
  %local.3 = alloca i32, align 4
  %local.4 = alloca i32, align 4
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  store ptr %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.2, align 8
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %v.1 = load i32, ptr %tmp.0, align 4
  store i32 %v.1, ptr %local.3, align 4
  %v.2 = load ptr, ptr %local.2, align 8
  %tmp.1 = getelementptr { i32 }, ptr %v.2, i32 0, i32 0
  %v.3 = load i32, ptr %tmp.1, align 4
  %v.4 = load i32, ptr %local.3, align 4
  %v.5 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.6, ptr %local.3, align 4
  %tmp.4 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store i32 %v.6, ptr %tmp.4, align 4
  %v.7 = add i32 0, 0
  store i32 %v.7, ptr %local.4, align 4
  %v.8 = load { ptr, i64 }, ptr %local.0, align 8
  %v.9 = load i32, ptr %local.4, align 4
  %tmp.5 = extractvalue { ptr, i64 } %v.8, 0
  %tmp.6 = extractvalue { ptr, i64 } %v.8, 1
  %tmp.7 = sext i32 %v.9 to i64
  %tmp.8 = icmp uge i64 %tmp.7, %tmp.6
  br i1 %tmp.8, label %zil.trap, label %checked.1
checked.1:
  %v.10 = getelementptr i8, ptr %tmp.5, i64 %tmp.7
  %v.11 = load { ptr, i64 }, ptr %local.0, align 8
  %v.12 = load i32, ptr %local.4, align 4
  %tmp.9 = extractvalue { ptr, i64 } %v.11, 0
  %tmp.10 = extractvalue { ptr, i64 } %v.11, 1
  %tmp.11 = sext i32 %v.12 to i64
  %tmp.12 = icmp uge i64 %tmp.11, %tmp.10
  br i1 %tmp.12, label %zil.trap, label %checked.2
checked.2:
  %v.13 = getelementptr i8, ptr %tmp.9, i64 %tmp.11
  %v.14 = getelementptr i8, ptr %v.13, i64 0
  %v.15 = load i8, ptr %v.14, align 1
  %v.16 = load i32, ptr %local.3, align 4
  %tmp.13 = icmp ugt i32 %v.16, 255
  br i1 %tmp.13, label %zil.trap, label %checked.3
checked.3:
  %v.17 = trunc i32 %v.16 to i8
  %tmp.14 = call { i8, i1 } @llvm.uadd.with.overflow.i8(i8 %v.15, i8 %v.17)
  %v.18 = extractvalue { i8, i1 } %tmp.14, 0
  %tmp.15 = extractvalue { i8, i1 } %tmp.14, 1
  br i1 %tmp.15, label %zil.trap, label %checked.4
checked.4:
  store i8 %v.18, ptr %v.10, align 1
  %v.19 = load ptr, ptr %local.1, align 8
  %v.20 = load ptr, ptr %local.1, align 8
  %v.21 = load i32, ptr %v.20, align 4
  %v.22 = load { ptr, i64 }, ptr %local.0, align 8
  %v.23 = add i32 0, 0
  %tmp.16 = extractvalue { ptr, i64 } %v.22, 0
  %tmp.17 = extractvalue { ptr, i64 } %v.22, 1
  %tmp.18 = sext i32 %v.23 to i64
  %tmp.19 = icmp uge i64 %tmp.18, %tmp.17
  br i1 %tmp.19, label %zil.trap, label %checked.5
checked.5:
  %v.24 = getelementptr i8, ptr %tmp.16, i64 %tmp.18
  %v.25 = getelementptr i8, ptr %v.24, i64 0
  %v.26 = load i8, ptr %v.25, align 1
  %v.27 = zext i8 %v.26 to i32
  %tmp.20 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.21, i32 %v.27)
  %v.28 = extractvalue { i32, i1 } %tmp.20, 0
  %tmp.21 = extractvalue { i32, i1 } %tmp.20, 1
  br i1 %tmp.21, label %zil.trap, label %checked.6
checked.6:
  store i32 %v.28, ptr %v.19, align 4
  %v.29 = load ptr, ptr %local.1, align 8
  %v.30 = load i32, ptr %v.29, align 4
  ret i32 %v.30
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { %zil_env_0, { ptr, i64, i64 }, i32 } @zil.fn.5(%zil_env_0 %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca %zil_env_0, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { ptr, i64, i64 }, align 8
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca %zil_env_0, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { ptr, i64, i64 }, align 8
  store %zil_env_0 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  %v.0 = load %zil_env_0, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.1, align 1
  store %zil_env_0 %v.0, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.2, align 1
  store { ptr, i64, i64 } %v.1, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.2 = add i32 0, 0
  store i32 %v.2, ptr %local.3, align 4
  %v.3 = load %zil_env_0, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store %zil_env_0 %v.3, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.4 = load { ptr, i64, i64 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store { ptr, i64, i64 } %v.4, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.5 = load i32, ptr %local.3, align 4
  %v.6 = load %zil_env_0, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  %v.7 = load { ptr, i64, i64 }, ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
%tmp.0 = insertvalue { %zil_env_0, { ptr, i64, i64 }, i32 } zeroinitializer, %zil_env_0 %v.6, 0
%tmp.1 = insertvalue { %zil_env_0, { ptr, i64, i64 }, i32 } %tmp.0, { ptr, i64, i64 } %v.7, 1
%v.8 = insertvalue { %zil_env_0, { ptr, i64, i64 }, i32 } %tmp.1, i32 %v.5, 2
  %tmp.2 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.2, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.bytes.drop(ptr %local.5)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.3 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.3, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.0(ptr %local.4)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.4 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.4, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.bytes.drop(ptr %local.2)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.5 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.5, label %closure.drop.3, label %closure.done.3
closure.drop.3:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.closure.drop.0(ptr %local.1)
  br label %closure.done.3
closure.done.3:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.6 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.6, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret { %zil_env_0, { ptr, i64, i64 }, i32 } %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.6({ %zil_env_0, { ptr, i64, i64 }, i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { %zil_env_0, { ptr, i64, i64 }, i32 }, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { %zil_env_0, { ptr, i64, i64 }, i32 }, align 8
  %local.2 = alloca ptr, align 8
  %local.3 = alloca { ptr, i64 }, align 8
  %local.4 = alloca ptr, align 8
  %local.5 = alloca i32, align 4
  %local.6 = alloca i32, align 4
  store { %zil_env_0, { ptr, i64, i64 }, i32 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.1, align 1
  store { %zil_env_0, { ptr, i64, i64 }, i32 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = getelementptr { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.1, i32 0, i32 1
  %v.2 = add i64 0, 1
  %v.3 = call i1 @zil.bytes.reserve(ptr %v.1, i64 %v.2)
  %v.4 = xor i1 %v.3, true
  br i1 %v.4, label %bb.1, label %bb.2
bb.1:
  %v.5 = add i32 0, 90
  %tmp.0 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.1 = getelementptr { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.closure.drop.0(ptr %tmp.1)
  %tmp.2 = getelementptr { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.2)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.3 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.3, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.4 = getelementptr { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.closure.drop.0(ptr %tmp.4)
  %tmp.5 = getelementptr { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.5)
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.5
bb.2:
  br label %bb.3
bb.3:
  %v.6 = getelementptr { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.1, i32 0, i32 1
  %v.7 = add i8 0, 3
  %v.8 = call i1 @zil.bytes.push(ptr %v.6, i8 %v.7)
  %v.9 = getelementptr { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.1, i32 0, i32 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  store ptr %v.10, ptr %local.2, align 8
  %v.11 = getelementptr { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.1, i32 0, i32 1
  %tmp.6 = load ptr, ptr %v.11, align 8
  %tmp.7 = getelementptr { ptr, i64, i64 }, ptr %v.11, i32 0, i32 1
  %tmp.8 = load i64, ptr %tmp.7, align 8
  %tmp.9 = insertvalue { ptr, i64 } poison, ptr %tmp.6, 0
  %v.12 = insertvalue { ptr, i64 } %tmp.9, i64 %tmp.8, 1
  store { ptr, i64 } %v.12, ptr %local.3, align 8
  %v.13 = getelementptr { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.1, i32 0, i32 2
  %v.14 = getelementptr i8, ptr %v.13, i64 0
  store ptr %v.14, ptr %local.4, align 8
  %v.15 = load ptr, ptr %local.2, align 8
  %v.16 = load { ptr, i64 }, ptr %local.3, align 8
  %tmp.10 = extractvalue { ptr, i64 } %v.16, 0
  %v.17 = insertvalue { ptr, i64 } %v.16, ptr %tmp.10, 0
  %v.18 = load ptr, ptr %local.4, align 8
  %v.19 = getelementptr i8, ptr %v.18, i64 0
  %v.20 = call i32 @zil.fn.4(ptr %v.15, { ptr, i64 } %v.17, ptr %v.19)
  store i32 %v.20, ptr %local.5, align 4
  %v.21 = load ptr, ptr %local.2, align 8
  %v.22 = load { ptr, i64 }, ptr %local.3, align 8
  %tmp.11 = extractvalue { ptr, i64 } %v.22, 0
  %v.23 = insertvalue { ptr, i64 } %v.22, ptr %tmp.11, 0
  %v.24 = load ptr, ptr %local.4, align 8
  %v.25 = getelementptr i8, ptr %v.24, i64 0
  %v.26 = call i32 @zil.fn.4(ptr %v.21, { ptr, i64 } %v.23, ptr %v.25)
  store i32 %v.26, ptr %local.6, align 4
  %v.27 = load i32, ptr %local.5, align 4
  %v.28 = load i32, ptr %local.6, align 4
  %tmp.12 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.27, i32 %v.28)
  %v.29 = extractvalue { i32, i1 } %tmp.12, 0
  %tmp.13 = extractvalue { i32, i1 } %tmp.12, 1
  br i1 %tmp.13, label %zil.trap, label %checked.2
checked.2:
  %v.30 = load ptr, ptr %local.4, align 8
  %v.31 = load i32, ptr %v.30, align 4
  %tmp.14 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.29, i32 %v.31)
  %v.32 = extractvalue { i32, i1 } %tmp.14, 0
  %tmp.15 = extractvalue { i32, i1 } %tmp.14, 1
  br i1 %tmp.15, label %zil.trap, label %checked.3
checked.3:
  %tmp.16 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.16, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.17 = getelementptr { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.closure.drop.0(ptr %tmp.17)
  %tmp.18 = getelementptr { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.18)
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.19 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.19, label %package.drop.5, label %package.done.5
package.drop.5:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.20 = getelementptr { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.closure.drop.0(ptr %tmp.20)
  %tmp.21 = getelementptr { %zil_env_0, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.21)
  br label %package.done.5
package.done.5:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.32
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.3()
  ret i32 %entry.result
}
