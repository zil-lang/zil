; ZIL verified LLVM module
; verified-fingerprint: cf92da721027410b0f35e8b71616ad5743cc6509bd4abae0093e6d2873d5799d
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
  call void @zil.fn.2(ptr %payload.0)
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
  call void @zil.fn.2(ptr %payload.0)
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

declare i32 @getchar() nonlazybind
define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_0, align 8
  %local.3 = alloca { ptr, i64 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca %zil_env_1, align 8
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { { ptr, i64, i64 }, %zil_env_1, i64 }, align 8
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { { ptr, i64, i64 }, %zil_env_1, i64 }, align 8
  %local.7 = alloca i32, align 4
  %local.8 = alloca { ptr, i64 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.4, align 1
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = call i32 @getchar()
  store i32 %v.1, ptr %local.0, align 4
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = add i32 0, 48
  %v.4 = icmp slt i32 %v.2, %v.3
  br i1 %v.4, label %bb.1, label %bb.2
bb.1:
  %v.5 = or i1 false, true
  store i1 %v.5, ptr %local.1, align 1
  br label %bb.3
bb.2:
  %v.6 = load i32, ptr %local.0, align 4
  %v.7 = add i32 0, 51
  %v.8 = icmp sgt i32 %v.6, %v.7
  store i1 %v.8, ptr %local.1, align 1
  br label %bb.3
bb.3:
  %v.9 = load i1, ptr %local.1, align 1
  br i1 %v.9, label %bb.4, label %bb.5
bb.4:
  %v.10 = add i32 0, 9
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.6, align 1
  ret i32 %v.10
bb.5:
  br label %bb.6
bb.6:
  %v.11 = add i8 0, 65
  %v.12 = call %zil_env_0 @zil.fn.3(i8 %v.11)
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.12, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.13 = getelementptr i8, ptr %local.2, i64 0
  %v.14 = getelementptr i8, ptr %v.13, i64 0
  %v.15 = getelementptr i8, ptr %v.14, i64 0
  %v.16 = call { ptr, i64 } @zil.fn.9(ptr %v.15)
  store { ptr, i64 } %v.16, ptr %local.3, align 8
  %v.17 = load { ptr, i64 }, ptr %local.3, align 8
  %v.18 = extractvalue { ptr, i64 } %v.17, 1
  %v.19 = add i64 0, 1
  %v.20 = icmp ne i64 %v.18, %v.19
  br i1 %v.20, label %bb.7, label %bb.8
bb.7:
  %v.21 = add i32 0, 1
  %tmp.0 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.0, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.6, align 1
  ret i32 %v.21
bb.8:
  br label %bb.9
bb.9:
  %v.22 = load { ptr, i64 }, ptr %local.3, align 8
  %v.23 = add i32 0, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.22, 0
  %tmp.2 = extractvalue { ptr, i64 } %v.22, 1
  %tmp.3 = sext i32 %v.23 to i64
  %tmp.4 = icmp uge i64 %tmp.3, %tmp.2
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.24 = getelementptr i8, ptr %tmp.1, i64 %tmp.3
  %v.25 = getelementptr i8, ptr %v.24, i64 0
  %v.26 = load i8, ptr %v.25, align 1
  %v.27 = zext i8 %v.26 to i32
  call void @zil.fn.6(i32 %v.27)
  %v.29 = call %zil_env_1 @zil.fn.4()
  store %zil_env_1 %v.29, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.30 = load %zil_env_1, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  %v.31 = call { { ptr, i64, i64 }, %zil_env_1, i64 } @zil.fn.10(%zil_env_1 %v.30)
  %tmp.5 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.5, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.1(ptr %local.4)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.5, align 1
  store { { ptr, i64, i64 }, %zil_env_1, i64 } %v.31, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.32 = load { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
  store { { ptr, i64, i64 }, %zil_env_1, i64 } %v.32, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  %v.33 = load i32, ptr %local.0, align 4
  %v.34 = add i32 0, 48
  %tmp.6 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.33, i32 %v.34)
  %v.35 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
  %v.36 = sext i32 %v.35 to i64
  %v.37 = load { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  %v.38 = call i32 @zil.fn.11({ { ptr, i64, i64 }, %zil_env_1, i64 } %v.37, i64 %v.36)
  store i32 %v.38, ptr %local.7, align 4
  %tmp.8 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.8, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.9 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.6, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.9)
  %tmp.10 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.6, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.10)
  br label %package.done.4
package.done.4:
  %v.39 = load i32, ptr %local.7, align 4
  %v.40 = add i32 0, 0
  %v.41 = icmp ne i32 %v.39, %v.40
  br i1 %v.41, label %bb.10, label %bb.11
bb.10:
  %v.42 = load i32, ptr %local.7, align 4
  store i1 false, ptr %resource.live.6, align 1
  %tmp.11 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.11, label %package.drop.5, label %package.done.5
package.drop.5:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.12 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.5, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.12)
  %tmp.13 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.5, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.13)
  br label %package.done.5
package.done.5:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  %tmp.14 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.14, label %closure.drop.6, label %closure.done.6
closure.drop.6:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.6
closure.done.6:
  store i1 false, ptr %resource.live.2, align 1
  ret i32 %v.42
bb.11:
  br label %bb.12
bb.12:
  %v.43 = getelementptr i8, ptr %local.2, i64 0
  %v.44 = getelementptr i8, ptr %v.43, i64 0
  %v.45 = getelementptr i8, ptr %v.44, i64 0
  %v.46 = call { ptr, i64 } @zil.fn.9(ptr %v.45)
  store { ptr, i64 } %v.46, ptr %local.8, align 8
  %v.47 = load { ptr, i64 }, ptr %local.8, align 8
  %v.48 = add i32 0, 0
  %tmp.15 = extractvalue { ptr, i64 } %v.47, 0
  %tmp.16 = extractvalue { ptr, i64 } %v.47, 1
  %tmp.17 = sext i32 %v.48 to i64
  %tmp.18 = icmp uge i64 %tmp.17, %tmp.16
  br i1 %tmp.18, label %zil.trap, label %checked.7
checked.7:
  %v.49 = getelementptr i8, ptr %tmp.15, i64 %tmp.17
  %v.50 = getelementptr i8, ptr %v.49, i64 0
  %v.51 = load i8, ptr %v.50, align 1
  %v.52 = zext i8 %v.51 to i32
  call void @zil.fn.6(i32 %v.52)
  %v.54 = load { ptr, i64 }, ptr %local.3, align 8
  %v.55 = add i32 0, 0
  %tmp.19 = extractvalue { ptr, i64 } %v.54, 0
  %tmp.20 = extractvalue { ptr, i64 } %v.54, 1
  %tmp.21 = sext i32 %v.55 to i64
  %tmp.22 = icmp uge i64 %tmp.21, %tmp.20
  br i1 %tmp.22, label %zil.trap, label %checked.8
checked.8:
  %v.56 = getelementptr i8, ptr %tmp.19, i64 %tmp.21
  %v.57 = getelementptr i8, ptr %v.56, i64 0
  %v.58 = load i8, ptr %v.57, align 1
  %v.59 = load { ptr, i64 }, ptr %local.8, align 8
  %v.60 = add i32 0, 0
  %tmp.23 = extractvalue { ptr, i64 } %v.59, 0
  %tmp.24 = extractvalue { ptr, i64 } %v.59, 1
  %tmp.25 = sext i32 %v.60 to i64
  %tmp.26 = icmp uge i64 %tmp.25, %tmp.24
  br i1 %tmp.26, label %zil.trap, label %checked.9
checked.9:
  %v.61 = getelementptr i8, ptr %tmp.23, i64 %tmp.25
  %v.62 = getelementptr i8, ptr %v.61, i64 0
  %v.63 = load i8, ptr %v.62, align 1
  %v.64 = icmp ne i8 %v.58, %v.63
  br i1 %v.64, label %bb.13, label %bb.14
bb.13:
  %v.65 = add i32 0, 8
  store i1 false, ptr %resource.live.6, align 1
  %tmp.27 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.27, label %package.drop.10, label %package.done.10
package.drop.10:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.28 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.5, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.28)
  %tmp.29 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.5, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.29)
  br label %package.done.10
package.done.10:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  %tmp.30 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.30, label %closure.drop.11, label %closure.done.11
closure.drop.11:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.11
closure.done.11:
  store i1 false, ptr %resource.live.2, align 1
  ret i32 %v.65
bb.14:
  br label %bb.15
bb.15:
  %v.66 = add i32 0, 0
  store i1 false, ptr %resource.live.6, align 1
  %tmp.31 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.31, label %package.drop.12, label %package.done.12
package.drop.12:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.32 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.5, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.32)
  %tmp.33 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.5, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.33)
  br label %package.done.12
package.done.12:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  %tmp.34 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.34, label %closure.drop.13, label %closure.done.13
closure.drop.13:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.13
closure.done.13:
  store i1 false, ptr %resource.live.2, align 1
  ret i32 %v.66
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.2(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  call void @zil.fn.6(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.3(i8 %arg.0) {
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
  call void @zil.fn.2(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  ret %zil_env_0 %v.12
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_1 @zil.fn.4() {
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
  call void @zil.fn.2(ptr %local.1)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  ret %zil_env_1 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.6(i32 %arg.0) {
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

define { ptr, i64 } @zil.fn.7(ptr %env) {
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

define { ptr, i64 } @zil.fn.8(ptr %env, i8 %arg.0) {
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

define { ptr, i64 } @zil.fn.9(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = call { ptr, i64 } @zil.fn.7(ptr %v.0)
  ret { ptr, i64 } %v.1
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

define i32 @zil.fn.11({ { ptr, i64, i64 }, %zil_env_1, i64 } %arg.0, i64 %arg.1) {
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
  %tmp.1 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.2, i32 0, i32 2
  %v.7 = load i64, ptr %tmp.1, align 8
  %v.8 = add i64 0, 66
  %tmp.2 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.7, i64 %v.8)
  %v.9 = extractvalue { i64, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i64, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %tmp.4 = icmp ugt i64 %v.9, 255
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.10 = trunc i64 %v.9 to i8
  %v.11 = call { ptr, i64 } @zil.fn.8(ptr %v.6, i8 %v.10)
  store { ptr, i64 } %v.11, ptr %local.4, align 8
  %tmp.5 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.2, i32 0, i32 2
  %v.12 = load i64, ptr %tmp.5, align 8
  %v.13 = add i64 0, 1
  %tmp.6 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.12, i64 %v.13)
  %v.14 = extractvalue { i64, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i64, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.2
checked.2:
  %tmp.8 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.2, i32 0, i32 2
  store i64 %v.14, ptr %tmp.8, align 8
  %v.15 = load { ptr, i64 }, ptr %local.4, align 8
  %v.16 = extractvalue { ptr, i64 } %v.15, 1
  %tmp.9 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.2, i32 0, i32 2
  %v.17 = load i64, ptr %tmp.9, align 8
  %v.18 = icmp ne i64 %v.16, %v.17
  br i1 %v.18, label %bb.4, label %bb.5
bb.3:
  %v.29 = add i32 0, 0
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
  ret i32 %v.29
bb.4:
  %v.19 = add i32 0, 1
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
  ret i32 %v.19
bb.5:
  br label %bb.6
bb.6:
  %v.20 = load { ptr, i64 }, ptr %local.4, align 8
  %tmp.26 = getelementptr { { ptr, i64, i64 }, %zil_env_1, i64 }, ptr %local.2, i32 0, i32 2
  %v.21 = load i64, ptr %tmp.26, align 8
  %v.22 = add i64 0, 1
  %tmp.27 = call { i64, i1 } @llvm.ssub.with.overflow.i64(i64 %v.21, i64 %v.22)
  %v.23 = extractvalue { i64, i1 } %tmp.27, 0
  %tmp.28 = extractvalue { i64, i1 } %tmp.27, 1
  br i1 %tmp.28, label %zil.trap, label %checked.11
checked.11:
  %tmp.29 = extractvalue { ptr, i64 } %v.20, 0
  %tmp.30 = extractvalue { ptr, i64 } %v.20, 1
  %tmp.31 = icmp uge i64 %v.23, %tmp.30
  br i1 %tmp.31, label %zil.trap, label %checked.12
checked.12:
  %v.24 = getelementptr i8, ptr %tmp.29, i64 %v.23
  %v.25 = getelementptr i8, ptr %v.24, i64 0
  %v.26 = load i8, ptr %v.25, align 1
  %v.27 = zext i8 %v.26 to i32
  call void @zil.fn.6(i32 %v.27)
  br label %bb.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}
