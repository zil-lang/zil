; ZIL verified LLVM module
; verified-fingerprint: 8ea5368450703786b60fa32c84a2c94849610d8c681ad5a145b856512d489065
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32, { i32 }, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
  %live.1 = load i1, ptr %flag.ptr.1, align 1
  br i1 %live.1, label %drop.1, label %done.1
drop.1:
  store i1 false, ptr %flag.ptr.1, align 1
  %payload.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  call void @zil.fn.2(ptr %payload.1)
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
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define { i1, {}, i32 } @zil.fn.0() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } }, align 8
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %local.6 = alloca i1, align 1
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = call %zil_env_0 @zil.fn.9()
  store i1 false, ptr %resource.live.0, align 1
  store %zil_env_0 %v.0, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = call { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } @zil.fn.4()
  store i1 false, ptr %resource.live.1, align 1
  store { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } %v.1, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.2 = load { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } %v.2, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = load { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  %v.4 = call { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } @zil.fn.7({ i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } %v.3)
  store { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } %v.4, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %tmp.0 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.0, label %error.drop.0, label %error.done.0
error.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.1 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, ptr %local.3, i32 0, i32 0
  %tmp.2 = load i1, ptr %tmp.1, align 1
  br i1 %tmp.2, label %error.payload.0, label %success.payload.0
error.payload.0:
  %tmp.3 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, ptr %local.3, i32 0, i32 1
  %tmp.4 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.3, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.4)
  %tmp.5 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.3, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.5)
  br label %error.done.0
success.payload.0:
  br label %error.done.0
error.done.0:
  %tmp.6 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.6, label %error.drop.1, label %error.done.1
error.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.7 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, ptr %local.2, i32 0, i32 0
  %tmp.8 = load i1, ptr %tmp.7, align 1
  br i1 %tmp.8, label %error.payload.1, label %success.payload.1
error.payload.1:
  %tmp.9 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, ptr %local.2, i32 0, i32 1
  %tmp.10 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.9, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.10)
  %tmp.11 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.9, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.11)
  br label %error.done.1
success.payload.1:
  br label %error.done.1
error.done.1:
  %v.5 = call { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.5()
  store { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.5, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.6 = load { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  %v.7 = call { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.8({ i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.6)
  %v.8 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.7, 0
  br i1 %v.8, label %bb.1, label %bb.2
bb.1:
  %tmp.12 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.7, 0
  %tmp.13 = icmp ne i1 %tmp.12, true
  br i1 %tmp.13, label %zil.trap, label %checked.2
checked.2:
  %tmp.14 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.10 = insertvalue { i1, {}, i32 } %tmp.14, {} zeroinitializer, 1
  %tmp.15 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.15, label %error.drop.3, label %error.done.3
error.drop.3:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.16 = getelementptr { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } }, ptr %local.4, i32 0, i32 0
  %tmp.17 = load i1, ptr %tmp.16, align 1
  br i1 %tmp.17, label %error.payload.3, label %success.payload.3
error.payload.3:
  br label %error.done.3
success.payload.3:
  %tmp.18 = getelementptr { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } }, ptr %local.4, i32 0, i32 2
  %tmp.19 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.18, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.19)
  %tmp.20 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.18, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.20)
  br label %error.done.3
error.done.3:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.21 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.21, label %error.drop.4, label %error.done.4
error.drop.4:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.22 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, ptr %local.1, i32 0, i32 0
  %tmp.23 = load i1, ptr %tmp.22, align 1
  br i1 %tmp.23, label %error.payload.4, label %success.payload.4
error.payload.4:
  %tmp.24 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, ptr %local.1, i32 0, i32 1
  %tmp.25 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.24, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.25)
  %tmp.26 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.24, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.26)
  br label %error.done.4
success.payload.4:
  br label %error.done.4
error.done.4:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.27 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.27, label %closure.drop.5, label %closure.done.5
closure.drop.5:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.5
closure.done.5:
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.7, align 1
  ret { i1, {}, i32 } %v.10
bb.2:
  %tmp.28 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.7, 0
  %tmp.29 = icmp ne i1 %tmp.28, false
  br i1 %tmp.29, label %zil.trap, label %checked.6
checked.6:
  %v.11 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.7, 2
  %tmp.30 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.30, label %error.drop.7, label %error.done.7
error.drop.7:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.31 = getelementptr { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } }, ptr %local.4, i32 0, i32 0
  %tmp.32 = load i1, ptr %tmp.31, align 1
  br i1 %tmp.32, label %error.payload.7, label %success.payload.7
error.payload.7:
  br label %error.done.7
success.payload.7:
  %tmp.33 = getelementptr { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } }, ptr %local.4, i32 0, i32 2
  %tmp.34 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.33, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.34)
  %tmp.35 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.33, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.35)
  br label %error.done.7
error.done.7:
  store i1 false, ptr %resource.live.5, align 1
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.11, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.12 = call i32 @zil.fn.10(ptr %local.0)
  %v.13 = add i32 0, 1
  %v.14 = icmp ne i32 %v.12, %v.13
  br i1 %v.14, label %bb.3, label %bb.4
bb.3:
  %v.15 = or i1 false, true
  store i1 %v.15, ptr %local.6, align 1
  br label %bb.5
bb.4:
  %v.16 = call i32 @zil.fn.10(ptr %local.0)
  %v.17 = add i32 0, 2
  %v.18 = icmp ne i32 %v.16, %v.17
  store i1 %v.18, ptr %local.6, align 1
  br label %bb.5
bb.5:
  %v.19 = load i1, ptr %local.6, align 1
  br i1 %v.19, label %bb.6, label %bb.7
bb.6:
  %tmp.36 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.21 = insertvalue { i1, {}, i32 } %tmp.36, {} zeroinitializer, 1
  %tmp.37 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.37, label %package.drop.8, label %package.done.8
package.drop.8:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.38 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.5, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.38)
  %tmp.39 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.5, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.39)
  br label %package.done.8
package.done.8:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.40 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.40, label %error.drop.9, label %error.done.9
error.drop.9:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.41 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, ptr %local.1, i32 0, i32 0
  %tmp.42 = load i1, ptr %tmp.41, align 1
  br i1 %tmp.42, label %error.payload.9, label %success.payload.9
error.payload.9:
  %tmp.43 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, ptr %local.1, i32 0, i32 1
  %tmp.44 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.43, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.44)
  %tmp.45 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.43, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.45)
  br label %error.done.9
success.payload.9:
  br label %error.done.9
error.done.9:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.46 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.46, label %closure.drop.10, label %closure.done.10
closure.drop.10:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.10
closure.done.10:
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.7, align 1
  ret { i1, {}, i32 } %v.21
bb.7:
  br label %bb.8
bb.8:
  %v.22 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.22, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  %v.23 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, align 8
  store i1 false, ptr %resource.live.7, align 1
  %v.24 = call i32 @zil.fn.6({ { i32 }, { ptr, i64, i64 }, i32 } %v.23)
  %v.25 = add i32 0, 66
  %tmp.47 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.24, i32 %v.25)
  %v.26 = extractvalue { i32, i1 } %tmp.47, 0
  %tmp.48 = extractvalue { i32, i1 } %tmp.47, 1
  br i1 %tmp.48, label %zil.trap, label %checked.11
checked.11:
  %tmp.49 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.27 = insertvalue { i1, {}, i32 } %tmp.49, i32 %v.26, 2
  %tmp.50 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.50, label %package.drop.12, label %package.done.12
package.drop.12:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.51 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.51)
  %tmp.52 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.52)
  br label %package.done.12
package.done.12:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.53 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.53, label %package.drop.13, label %package.done.13
package.drop.13:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.54 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.5, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.54)
  %tmp.55 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.5, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.55)
  br label %package.done.13
package.done.13:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.56 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.56, label %error.drop.14, label %error.done.14
error.drop.14:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.57 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, ptr %local.1, i32 0, i32 0
  %tmp.58 = load i1, ptr %tmp.57, align 1
  br i1 %tmp.58, label %error.payload.14, label %success.payload.14
error.payload.14:
  %tmp.59 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, ptr %local.1, i32 0, i32 1
  %tmp.60 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.59, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.60)
  %tmp.61 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.59, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.61)
  br label %error.done.14
success.payload.14:
  br label %error.done.14
error.done.14:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.62 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.62, label %closure.drop.15, label %closure.done.15
closure.drop.15:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.15
closure.done.15:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.27
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.2(ptr %arg.0) {
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

define { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.3(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { ptr, i64, i64 }, align 8
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { ptr, i64, i64 }, align 8
  %local.5 = alloca i32, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { ptr, i64, i64 }, align 8
  store i32 %arg.0, ptr %local.0, align 4
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
  %v.5 = load i32, ptr %local.0, align 4
  %tmp.0 = icmp ugt i32 %v.5, 255
  br i1 %tmp.0, label %zil.trap, label %checked.0
checked.0:
  %v.6 = trunc i32 %v.5 to i8
  %v.7 = call i1 @zil.bytes.push(ptr %v.4, i8 %v.6)
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  %v.8 = load i32, ptr %local.0, align 4
  store i32 %v.8, ptr %local.2, align 4
  %v.9 = load i32, ptr %local.2, align 4
%v.10 = insertvalue { i32 } zeroinitializer, i32 %v.9, 0
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.10, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.11 = load { ptr, i64, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.11, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.12 = load i32, ptr %local.0, align 4
  store i32 %v.12, ptr %local.5, align 4
  %v.13 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.13, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.14 = load { ptr, i64, i64 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.14, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  %v.15 = load i32, ptr %local.5, align 4
  %v.16 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  %v.17 = load { ptr, i64, i64 }, ptr %local.7, align 8
  store i1 false, ptr %resource.live.7, align 1
%tmp.1 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } zeroinitializer, { i32 } %v.16, 0
%tmp.2 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } %tmp.1, { ptr, i64, i64 } %v.17, 1
%v.18 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } %tmp.2, i32 %v.15, 2
  %tmp.3 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.bytes.drop(ptr %local.7)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.4 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.4, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.2(ptr %local.6)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.5 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.5, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.bytes.drop(ptr %local.4)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.6 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.6, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.2(ptr %local.3)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.7 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.7, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.bytes.drop(ptr %local.1)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.1, align 1
  ret { { i32 }, { ptr, i64, i64 }, i32 } %v.18
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } @zil.fn.4() {
entry:
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 65
  %v.1 = call { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.3(i32 %v.0)
  %tmp.0 = insertvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } zeroinitializer, i1 true, 0
  %v.2 = insertvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } %tmp.0, { { i32 }, { ptr, i64, i64 }, i32 } %v.1, 1
  ret { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.5() {
entry:
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 66
  %v.1 = call { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.3(i32 %v.0)
  %tmp.0 = insertvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } zeroinitializer, i1 false, 0
  %v.2 = insertvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %tmp.0, { { i32 }, { ptr, i64, i64 }, i32 } %v.1, 2
  ret { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.6({ { i32 }, { ptr, i64, i64 }, i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  store { { i32 }, { ptr, i64, i64 }, i32 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 2
  %v.0 = load i32, ptr %tmp.0, align 4
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.2 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.2)
  %tmp.3 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.3)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } @zil.fn.7({ i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, align 8
  store { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %error.drop.0, label %error.done.0
error.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.1 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, ptr %local.0, i32 0, i32 0
  %tmp.2 = load i1, ptr %tmp.1, align 1
  br i1 %tmp.2, label %error.payload.0, label %success.payload.0
error.payload.0:
  %tmp.3 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 }, ptr %local.0, i32 0, i32 1
  %tmp.4 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.3, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.4)
  %tmp.5 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.3, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.5)
  br label %error.done.0
success.payload.0:
  br label %error.done.0
error.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.8({ i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } }, align 8
  store { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %error.drop.0, label %error.done.0
error.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.1 = getelementptr { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } }, ptr %local.0, i32 0, i32 0
  %tmp.2 = load i1, ptr %tmp.1, align 1
  br i1 %tmp.2, label %error.payload.0, label %success.payload.0
error.payload.0:
  br label %error.done.0
success.payload.0:
  %tmp.3 = getelementptr { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } }, ptr %local.0, i32 0, i32 2
  %tmp.4 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.3, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.4)
  %tmp.5 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.3, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.5)
  br label %error.done.0
error.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.9() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 67
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = add i32 0, 0
  store i32 %v.3, ptr %local.2, align 4
  %v.4 = load i32, ptr %local.2, align 4
  %v.5 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, i32 %v.4, 0
  %tmp.1 = insertvalue %zil_env_0 %tmp.0, { i32 } %v.5, 1
  %v.6 = insertvalue %zil_env_0 %tmp.1, i1 true, 2
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.2(ptr %local.1)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret %zil_env_0 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.10(ptr %env) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store ptr %v.1, ptr %local.1, align 8
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = add i32 0, 1
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.2, i32 %v.3)
  %v.4 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.4, ptr %local.0, align 4
  %tmp.3 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store i32 %v.4, ptr %tmp.3, align 4
  %v.5 = load i32, ptr %local.0, align 4
  %v.6 = load ptr, ptr %local.1, align 8
  %tmp.4 = getelementptr { i32 }, ptr %v.6, i32 0, i32 0
  %v.7 = load i32, ptr %tmp.4, align 4
  %tmp.5 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.1
checked.1:
  %v.9 = add i32 0, 67
  %tmp.7 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.8, i32 %v.9)
  %v.10 = extractvalue { i32, i1 } %tmp.7, 0
  %tmp.8 = extractvalue { i32, i1 } %tmp.7, 1
  br i1 %tmp.8, label %zil.trap, label %checked.2
checked.2:
  ret i32 %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, {}, i32 } @zil.fn.0()
  %entry.error = extractvalue { i1, {}, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, {}, i32 } %entry.result, 2
  ret i32 %entry.success
}
