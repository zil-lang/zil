; ZIL verified LLVM module
; verified-fingerprint: ba08f57f2e349dce7211605cbd6232fc306167cae8139cdcda61d5f56139ed0b
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32, { i32 }, i32, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 3
  %live.1 = load i1, ptr %flag.ptr.1, align 1
  br i1 %live.1, label %drop.1, label %done.1
drop.1:
  store i1 false, ptr %flag.ptr.1, align 1
  %payload.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  call void @zil.fn.3(ptr %payload.1)
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

define internal i1 @zil.store.3.reserve(ptr %head, i64 %requested) {
entry:
  %stride = ptrtoint ptr getelementptr ({ { i32 }, { ptr, i64, i64 }, i32 }, ptr null, i64 1) to i64
  %maximum = udiv i64 9223372036854775807, %stride
  %positive = icmp sge i64 %requested, 0
  %bounded = icmp ule i64 %requested, %maximum
  %valid = and i1 %positive, %bounded
  br i1 %valid, label %capacity, label %failed
capacity:
  %cap.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 2
  %cap = load i64, ptr %cap.field, align 8
  %fits = icmp ule i64 %requested, %cap
  br i1 %fits, label %done, label %prepare
prepare:
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %old = load ptr, ptr %ptr.field, align 8
  %size = mul i64 %requested, %stride
  %next = call ptr @zil.bytes.reallocate(ptr %old, i64 %size)
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
define internal { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } @zil.store.3.push(ptr %head, { { i32 }, { ptr, i64, i64 }, i32 } %item) {
entry:
  %len.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 1
  %cap.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 2
  %len = load i64, ptr %len.field, align 8
  %cap = load i64, ptr %cap.field, align 8
  %room = icmp ult i64 %len, %cap
  br i1 %room, label %initialize, label %full
initialize:
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %data = load ptr, ptr %ptr.field, align 8
  %slot = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %data, i64 %len
  store { { i32 }, { ptr, i64, i64 }, i32 } %item, ptr %slot, align 8
  %next.len = add i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  ret { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } zeroinitializer
full:
  %tag = insertvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } zeroinitializer, i1 true, 0
  %error = insertvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } %tag, { { i32 }, { ptr, i64, i64 }, i32 } %item, 1
  ret { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } %error
}
define internal { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.store.3.pop(ptr %head) {
entry:
  %len.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 1
  %len = load i64, ptr %len.field, align 8
  %live = icmp ne i64 %len, 0
  br i1 %live, label %take, label %empty
take:
  %next.len = sub i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %data = load ptr, ptr %ptr.field, align 8
  %slot = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %data, i64 %next.len
  %item = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %slot, align 8
  %result = insertvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } zeroinitializer, { { i32 }, { ptr, i64, i64 }, i32 } %item, 2
  ret { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %result
empty:
  %error = insertvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } zeroinitializer, i1 true, 0
  ret { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %error
}
define internal void @zil.store.3.drop(ptr %head) {
entry:
  %len.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 1
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %len = load i64, ptr %len.field, align 8
  %data = load ptr, ptr %ptr.field, align 8
  store i64 0, ptr %len.field, align 8
  br label %loop
loop:
  %index = phi i64 [0, %entry], [%next, %element]
  %remaining = icmp ult i64 %index, %len
  br i1 %remaining, label %element, label %release
element:
  %slot = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %data, i64 %index
  %field.0 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %slot, i32 0, i32 0
  call void @zil.fn.3(ptr %field.0)
  %field.1 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %slot, i32 0, i32 1
  call void @zil.bytes.drop(ptr %field.1)
  %next = add i64 %index, 1
  br label %loop
release:
  call void @zil.bytes.release(ptr %data)
  ret void
}
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i32, i1 } @llvm.smul.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @getchar() nonlazybind
define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i1, align 1
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %local.4 = alloca i32, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca %zil_env_0, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  %package.live.6.0 = alloca i1, align 1
  store i1 false, ptr %package.live.6.0, align 1
  %package.live.6.1 = alloca i1, align 1
  store i1 false, ptr %package.live.6.1, align 1
  %package.live.6.2 = alloca i1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  %local.7 = alloca i32, align 4
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  %resource.live.9 = alloca i1, align 1
  store i1 false, ptr %resource.live.9, align 1
  %local.9 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  %resource.live.10 = alloca i1, align 1
  store i1 false, ptr %resource.live.10, align 1
  %local.10 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  %resource.live.11 = alloca i1, align 1
  store i1 false, ptr %resource.live.11, align 1
  %local.11 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  %local.12 = alloca i1, align 1
  %resource.live.13 = alloca i1, align 1
  store i1 false, ptr %resource.live.13, align 1
  %local.13 = alloca %zil_env_0, align 4
  %resource.live.14 = alloca i1, align 1
  store i1 false, ptr %resource.live.14, align 1
  %local.14 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  %resource.live.15 = alloca i1, align 1
  store i1 false, ptr %resource.live.15, align 1
  %local.15 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  %resource.live.16 = alloca i1, align 1
  store i1 false, ptr %resource.live.16, align 1
  %local.16 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.16, align 1
  store i1 false, ptr %resource.live.15, align 1
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.13, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %resource.live.5, align 1
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
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.13, align 1
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.15, align 1
  store i1 false, ptr %resource.live.16, align 1
  ret i32 %v.10
bb.5:
  br label %bb.6
bb.6:
  %v.11 = load i32, ptr %local.0, align 4
  %v.12 = add i32 0, 48
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.11, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.13, ptr %local.2, align 4
  %v.14 = load i32, ptr %local.2, align 4
%v.15 = insertvalue { i32 } zeroinitializer, i32 %v.14, 0
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.15, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.16 = getelementptr i8, ptr %local.3, i64 0
  %v.17 = getelementptr i8, ptr %v.16, i64 0
  %v.18 = call i32 @zil.fn.5(ptr %v.17)
  store i32 %v.18, ptr %local.4, align 4
  %v.19 = add i32 0, 72
  %v.20 = call %zil_env_0 @zil.fn.7(i32 %v.19)
  store %zil_env_0 %v.20, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.21 = load i32, ptr %local.4, align 4
  %v.22 = load %zil_env_0, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
  %v.23 = call { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } @zil.fn.14(%zil_env_0 %v.22, i32 %v.21)
  %tmp.2 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.2, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.closure.drop.0(ptr %local.5)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.23, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  store i1 true, ptr %package.live.6.0, align 1
  store i1 true, ptr %package.live.6.1, align 1
  store i1 true, ptr %package.live.6.2, align 1
  %tmp.3 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 5
  %v.24 = load i1, ptr %tmp.3, align 1
  %v.25 = xor i1 %v.24, true
  br i1 %v.25, label %bb.7, label %bb.8
bb.7:
  %v.26 = add i32 0, 1
  %tmp.4 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.4, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.5 = load i1, ptr %package.live.6.0, align 1
  br i1 %tmp.5, label %package.field.drop.3, label %package.field.done.3
package.field.drop.3:
  store i1 false, ptr %package.live.6.0, align 1
  %tmp.6 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.6)
  br label %package.field.done.3
package.field.done.3:
  %tmp.7 = load i1, ptr %package.live.6.1, align 1
  br i1 %tmp.7, label %package.field.drop.4, label %package.field.done.4
package.field.drop.4:
  store i1 false, ptr %package.live.6.1, align 1
  %tmp.8 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.8)
  br label %package.field.done.4
package.field.done.4:
  %tmp.9 = load i1, ptr %package.live.6.2, align 1
  br i1 %tmp.9, label %package.field.drop.5, label %package.field.done.5
package.field.drop.5:
  store i1 false, ptr %package.live.6.2, align 1
  %tmp.10 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.10)
  br label %package.field.done.5
package.field.done.5:
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.11 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.11, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.13, align 1
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.15, align 1
  store i1 false, ptr %resource.live.16, align 1
  ret i32 %v.26
bb.8:
  br label %bb.9
bb.9:
  %v.27 = load i32, ptr %local.4, align 4
  %v.28 = add i32 0, 68
  %tmp.12 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.27, i32 %v.28)
  %v.29 = extractvalue { i32, i1 } %tmp.12, 0
  %tmp.13 = extractvalue { i32, i1 } %tmp.12, 1
  br i1 %tmp.13, label %zil.trap, label %checked.7
checked.7:
  %v.30 = load i32, ptr %local.4, align 4
  %v.31 = load i32, ptr %local.4, align 4
  %v.32 = add i32 0, 1
  %tmp.14 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.31, i32 %v.32)
  %v.33 = extractvalue { i32, i1 } %tmp.14, 0
  %tmp.15 = extractvalue { i32, i1 } %tmp.14, 1
  br i1 %tmp.15, label %zil.trap, label %checked.8
checked.8:
  %tmp.16 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.30, i32 %v.33)
  %v.34 = extractvalue { i32, i1 } %tmp.16, 0
  %tmp.17 = extractvalue { i32, i1 } %tmp.16, 1
  br i1 %tmp.17, label %zil.trap, label %checked.9
checked.9:
  %v.35 = add i32 0, 2
  %tmp.18 = icmp eq i32 %v.35, 0
  %tmp.19 = icmp eq i32 %v.34, -2147483648
  %tmp.20 = icmp eq i32 %v.35, -1
  %tmp.21 = and i1 %tmp.19, %tmp.20
  %tmp.22 = or i1 %tmp.18, %tmp.21
  br i1 %tmp.22, label %zil.trap, label %checked.10
checked.10:
  %v.36 = sdiv i32 %v.34, %v.35
  %tmp.23 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.29, i32 %v.36)
  %v.37 = extractvalue { i32, i1 } %tmp.23, 0
  %tmp.24 = extractvalue { i32, i1 } %tmp.23, 1
  br i1 %tmp.24, label %zil.trap, label %checked.11
checked.11:
  store i32 %v.37, ptr %local.7, align 4
  %tmp.25 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 4
  %v.38 = load i32, ptr %tmp.25, align 4
  %v.39 = load i32, ptr %local.7, align 4
  %v.40 = icmp ne i32 %v.38, %v.39
  br i1 %v.40, label %bb.10, label %bb.11
bb.10:
  %v.41 = add i32 0, 2
  %tmp.26 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.26, label %package.drop.12, label %package.done.12
package.drop.12:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.27 = load i1, ptr %package.live.6.0, align 1
  br i1 %tmp.27, label %package.field.drop.13, label %package.field.done.13
package.field.drop.13:
  store i1 false, ptr %package.live.6.0, align 1
  %tmp.28 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.28)
  br label %package.field.done.13
package.field.done.13:
  %tmp.29 = load i1, ptr %package.live.6.1, align 1
  br i1 %tmp.29, label %package.field.drop.14, label %package.field.done.14
package.field.drop.14:
  store i1 false, ptr %package.live.6.1, align 1
  %tmp.30 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.30)
  br label %package.field.done.14
package.field.done.14:
  %tmp.31 = load i1, ptr %package.live.6.2, align 1
  br i1 %tmp.31, label %package.field.drop.15, label %package.field.done.15
package.field.drop.15:
  store i1 false, ptr %package.live.6.2, align 1
  %tmp.32 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.32)
  br label %package.field.done.15
package.field.done.15:
  br label %package.done.12
package.done.12:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.33 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.33, label %resource.drop.16, label %resource.done.16
resource.drop.16:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.16
resource.done.16:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.13, align 1
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.15, align 1
  store i1 false, ptr %resource.live.16, align 1
  ret i32 %v.41
bb.11:
  br label %bb.12
bb.12:
  %v.42 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.42, ptr %local.8, align 8
  store i1 true, ptr %resource.live.8, align 1
  %v.43 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.8, align 8
  store i1 false, ptr %resource.live.8, align 1
  %v.44 = call { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } @zil.fn.15({ { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.43)
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.44, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  store i1 true, ptr %package.live.6.0, align 1
  store i1 true, ptr %package.live.6.1, align 1
  store i1 true, ptr %package.live.6.2, align 1
  %tmp.34 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.34, label %package.drop.17, label %package.done.17
package.drop.17:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.35 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.8, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.35)
  %tmp.36 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.8, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.36)
  %tmp.37 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.8, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.37)
  br label %package.done.17
package.done.17:
  %tmp.38 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 3
  %v.45 = load i32, ptr %tmp.38, align 4
  %v.46 = add i32 0, 1
  %v.47 = icmp ne i32 %v.45, %v.46
  br i1 %v.47, label %bb.13, label %bb.14
bb.13:
  %v.48 = add i32 0, 3
  store i1 false, ptr %resource.live.8, align 1
  %tmp.39 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.39, label %package.drop.18, label %package.done.18
package.drop.18:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.40 = load i1, ptr %package.live.6.0, align 1
  br i1 %tmp.40, label %package.field.drop.19, label %package.field.done.19
package.field.drop.19:
  store i1 false, ptr %package.live.6.0, align 1
  %tmp.41 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.41)
  br label %package.field.done.19
package.field.done.19:
  %tmp.42 = load i1, ptr %package.live.6.1, align 1
  br i1 %tmp.42, label %package.field.drop.20, label %package.field.done.20
package.field.drop.20:
  store i1 false, ptr %package.live.6.1, align 1
  %tmp.43 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.43)
  br label %package.field.done.20
package.field.done.20:
  %tmp.44 = load i1, ptr %package.live.6.2, align 1
  br i1 %tmp.44, label %package.field.drop.21, label %package.field.done.21
package.field.drop.21:
  store i1 false, ptr %package.live.6.2, align 1
  %tmp.45 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.45)
  br label %package.field.done.21
package.field.done.21:
  br label %package.done.18
package.done.18:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.46 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.46, label %resource.drop.22, label %resource.done.22
resource.drop.22:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.22
resource.done.22:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.13, align 1
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.15, align 1
  store i1 false, ptr %resource.live.16, align 1
  ret i32 %v.48
bb.14:
  br label %bb.15
bb.15:
  %v.49 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.49, ptr %local.9, align 8
  store i1 true, ptr %resource.live.9, align 1
  %v.50 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.9, align 8
  store i1 false, ptr %resource.live.9, align 1
  %v.51 = call { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } @zil.fn.15({ { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.50)
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.51, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  store i1 true, ptr %package.live.6.0, align 1
  store i1 true, ptr %package.live.6.1, align 1
  store i1 true, ptr %package.live.6.2, align 1
  %tmp.47 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.47, label %package.drop.23, label %package.done.23
package.drop.23:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.48 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.9, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.48)
  %tmp.49 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.9, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.49)
  %tmp.50 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.9, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.50)
  br label %package.done.23
package.done.23:
  %tmp.51 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 3
  %v.52 = load i32, ptr %tmp.51, align 4
  %v.53 = add i32 0, 2
  %v.54 = icmp ne i32 %v.52, %v.53
  br i1 %v.54, label %bb.16, label %bb.17
bb.16:
  %v.55 = add i32 0, 4
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.8, align 1
  %tmp.52 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.52, label %package.drop.24, label %package.done.24
package.drop.24:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.53 = load i1, ptr %package.live.6.0, align 1
  br i1 %tmp.53, label %package.field.drop.25, label %package.field.done.25
package.field.drop.25:
  store i1 false, ptr %package.live.6.0, align 1
  %tmp.54 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.54)
  br label %package.field.done.25
package.field.done.25:
  %tmp.55 = load i1, ptr %package.live.6.1, align 1
  br i1 %tmp.55, label %package.field.drop.26, label %package.field.done.26
package.field.drop.26:
  store i1 false, ptr %package.live.6.1, align 1
  %tmp.56 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.56)
  br label %package.field.done.26
package.field.done.26:
  %tmp.57 = load i1, ptr %package.live.6.2, align 1
  br i1 %tmp.57, label %package.field.drop.27, label %package.field.done.27
package.field.drop.27:
  store i1 false, ptr %package.live.6.2, align 1
  %tmp.58 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.58)
  br label %package.field.done.27
package.field.done.27:
  br label %package.done.24
package.done.24:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.59 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.59, label %resource.drop.28, label %resource.done.28
resource.drop.28:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.28
resource.done.28:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.13, align 1
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.15, align 1
  store i1 false, ptr %resource.live.16, align 1
  ret i32 %v.55
bb.17:
  br label %bb.18
bb.18:
  %v.56 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.56, ptr %local.10, align 8
  store i1 true, ptr %resource.live.10, align 1
  %v.57 = load i32, ptr %local.4, align 4
  %v.58 = add i32 0, 1
  %tmp.60 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.57, i32 %v.58)
  %v.59 = extractvalue { i32, i1 } %tmp.60, 0
  %tmp.61 = extractvalue { i32, i1 } %tmp.60, 1
  br i1 %tmp.61, label %zil.trap, label %checked.29
checked.29:
  %v.60 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.10, align 8
  store i1 false, ptr %resource.live.10, align 1
  %v.61 = call { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } @zil.fn.16({ { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.60, i32 %v.59)
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.61, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  store i1 true, ptr %package.live.6.0, align 1
  store i1 true, ptr %package.live.6.1, align 1
  store i1 true, ptr %package.live.6.2, align 1
  %tmp.62 = load i1, ptr %resource.live.10, align 1
  br i1 %tmp.62, label %package.drop.30, label %package.done.30
package.drop.30:
  store i1 false, ptr %resource.live.10, align 1
  %tmp.63 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.10, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.63)
  %tmp.64 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.10, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.64)
  %tmp.65 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.10, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.65)
  br label %package.done.30
package.done.30:
  %tmp.66 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 5
  %v.62 = load i1, ptr %tmp.66, align 1
  %v.63 = xor i1 %v.62, true
  br i1 %v.63, label %bb.19, label %bb.20
bb.19:
  store i1 false, ptr %resource.live.11, align 1
  %v.64 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.64, ptr %local.11, align 8
  store i1 true, ptr %resource.live.11, align 1
  %v.65 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.11, align 8
  store i1 false, ptr %resource.live.11, align 1
  %v.66 = call { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } @zil.fn.15({ { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.65)
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.66, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  store i1 true, ptr %package.live.6.0, align 1
  store i1 true, ptr %package.live.6.1, align 1
  store i1 true, ptr %package.live.6.2, align 1
  %tmp.67 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.67, label %package.drop.31, label %package.done.31
package.drop.31:
  store i1 false, ptr %resource.live.11, align 1
  %tmp.68 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.11, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.68)
  %tmp.69 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.11, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.69)
  %tmp.70 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.11, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.70)
  br label %package.done.31
package.done.31:
  %tmp.71 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 3
  %v.67 = load i32, ptr %tmp.71, align 4
  %v.68 = add i32 0, 3
  %v.69 = icmp ne i32 %v.67, %v.68
  br i1 %v.69, label %bb.21, label %bb.22
bb.20:
  br label %bb.27
bb.21:
  %v.70 = or i1 false, true
  store i1 %v.70, ptr %local.12, align 1
  br label %bb.23
bb.22:
  %tmp.72 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 4
  %v.71 = load i32, ptr %tmp.72, align 4
  %v.72 = load i32, ptr %local.7, align 4
  %v.73 = icmp ne i32 %v.71, %v.72
  store i1 %v.73, ptr %local.12, align 1
  br label %bb.23
bb.23:
  %v.74 = load i1, ptr %local.12, align 1
  br i1 %v.74, label %bb.24, label %bb.25
bb.24:
  %v.75 = add i32 0, 5
  store i1 false, ptr %resource.live.11, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.8, align 1
  %tmp.73 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.73, label %package.drop.32, label %package.done.32
package.drop.32:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.74 = load i1, ptr %package.live.6.0, align 1
  br i1 %tmp.74, label %package.field.drop.33, label %package.field.done.33
package.field.drop.33:
  store i1 false, ptr %package.live.6.0, align 1
  %tmp.75 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.75)
  br label %package.field.done.33
package.field.done.33:
  %tmp.76 = load i1, ptr %package.live.6.1, align 1
  br i1 %tmp.76, label %package.field.drop.34, label %package.field.done.34
package.field.drop.34:
  store i1 false, ptr %package.live.6.1, align 1
  %tmp.77 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.77)
  br label %package.field.done.34
package.field.done.34:
  %tmp.78 = load i1, ptr %package.live.6.2, align 1
  br i1 %tmp.78, label %package.field.drop.35, label %package.field.done.35
package.field.drop.35:
  store i1 false, ptr %package.live.6.2, align 1
  %tmp.79 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.79)
  br label %package.field.done.35
package.field.done.35:
  br label %package.done.32
package.done.32:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.80 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.80, label %resource.drop.36, label %resource.done.36
resource.drop.36:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.36
resource.done.36:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.13, align 1
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.15, align 1
  store i1 false, ptr %resource.live.16, align 1
  ret i32 %v.75
bb.25:
  br label %bb.26
bb.26:
  %v.76 = add i32 0, 1
  store i1 false, ptr %resource.live.11, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.8, align 1
  %tmp.81 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.81, label %package.drop.37, label %package.done.37
package.drop.37:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.82 = load i1, ptr %package.live.6.0, align 1
  br i1 %tmp.82, label %package.field.drop.38, label %package.field.done.38
package.field.drop.38:
  store i1 false, ptr %package.live.6.0, align 1
  %tmp.83 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.83)
  br label %package.field.done.38
package.field.done.38:
  %tmp.84 = load i1, ptr %package.live.6.1, align 1
  br i1 %tmp.84, label %package.field.drop.39, label %package.field.done.39
package.field.drop.39:
  store i1 false, ptr %package.live.6.1, align 1
  %tmp.85 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.85)
  br label %package.field.done.39
package.field.done.39:
  %tmp.86 = load i1, ptr %package.live.6.2, align 1
  br i1 %tmp.86, label %package.field.drop.40, label %package.field.done.40
package.field.drop.40:
  store i1 false, ptr %package.live.6.2, align 1
  %tmp.87 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.87)
  br label %package.field.done.40
package.field.done.40:
  br label %package.done.37
package.done.37:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.88 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.88, label %resource.drop.41, label %resource.done.41
resource.drop.41:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.41
resource.done.41:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.13, align 1
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.15, align 1
  store i1 false, ptr %resource.live.16, align 1
  ret i32 %v.76
bb.27:
  %tmp.89 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 4
  %v.77 = load i32, ptr %tmp.89, align 4
  %v.78 = load i32, ptr %local.4, align 4
  %v.79 = add i32 0, 1
  %tmp.90 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.78, i32 %v.79)
  %v.80 = extractvalue { i32, i1 } %tmp.90, 0
  %tmp.91 = extractvalue { i32, i1 } %tmp.90, 1
  br i1 %tmp.91, label %zil.trap, label %checked.42
checked.42:
  %v.81 = add i32 0, 77
  %tmp.92 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.80, i32 %v.81)
  %v.82 = extractvalue { i32, i1 } %tmp.92, 0
  %tmp.93 = extractvalue { i32, i1 } %tmp.92, 1
  br i1 %tmp.93, label %zil.trap, label %checked.43
checked.43:
  %v.83 = load i32, ptr %local.4, align 4
  %v.84 = load i32, ptr %local.4, align 4
  %v.85 = add i32 0, 1
  %tmp.94 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.84, i32 %v.85)
  %v.86 = extractvalue { i32, i1 } %tmp.94, 0
  %tmp.95 = extractvalue { i32, i1 } %tmp.94, 1
  br i1 %tmp.95, label %zil.trap, label %checked.44
checked.44:
  %tmp.96 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.83, i32 %v.86)
  %v.87 = extractvalue { i32, i1 } %tmp.96, 0
  %tmp.97 = extractvalue { i32, i1 } %tmp.96, 1
  br i1 %tmp.97, label %zil.trap, label %checked.45
checked.45:
  %v.88 = add i32 0, 2
  %tmp.98 = icmp eq i32 %v.88, 0
  %tmp.99 = icmp eq i32 %v.87, -2147483648
  %tmp.100 = icmp eq i32 %v.88, -1
  %tmp.101 = and i1 %tmp.99, %tmp.100
  %tmp.102 = or i1 %tmp.98, %tmp.101
  br i1 %tmp.102, label %zil.trap, label %checked.46
checked.46:
  %v.89 = sdiv i32 %v.87, %v.88
  %tmp.103 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.82, i32 %v.89)
  %v.90 = extractvalue { i32, i1 } %tmp.103, 0
  %tmp.104 = extractvalue { i32, i1 } %tmp.103, 1
  br i1 %tmp.104, label %zil.trap, label %checked.47
checked.47:
  %v.91 = icmp ne i32 %v.77, %v.90
  br i1 %v.91, label %bb.28, label %bb.29
bb.28:
  %v.92 = add i32 0, 6
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.8, align 1
  %tmp.105 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.105, label %package.drop.48, label %package.done.48
package.drop.48:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.106 = load i1, ptr %package.live.6.0, align 1
  br i1 %tmp.106, label %package.field.drop.49, label %package.field.done.49
package.field.drop.49:
  store i1 false, ptr %package.live.6.0, align 1
  %tmp.107 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.107)
  br label %package.field.done.49
package.field.done.49:
  %tmp.108 = load i1, ptr %package.live.6.1, align 1
  br i1 %tmp.108, label %package.field.drop.50, label %package.field.done.50
package.field.drop.50:
  store i1 false, ptr %package.live.6.1, align 1
  %tmp.109 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.109)
  br label %package.field.done.50
package.field.done.50:
  %tmp.110 = load i1, ptr %package.live.6.2, align 1
  br i1 %tmp.110, label %package.field.drop.51, label %package.field.done.51
package.field.drop.51:
  store i1 false, ptr %package.live.6.2, align 1
  %tmp.111 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.111)
  br label %package.field.done.51
package.field.done.51:
  br label %package.done.48
package.done.48:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.112 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.112, label %resource.drop.52, label %resource.done.52
resource.drop.52:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.52
resource.done.52:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.13, align 1
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.15, align 1
  store i1 false, ptr %resource.live.16, align 1
  ret i32 %v.92
bb.29:
  br label %bb.30
bb.30:
  %v.93 = add i32 0, 75
  %v.94 = call %zil_env_0 @zil.fn.7(i32 %v.93)
  store %zil_env_0 %v.94, ptr %local.13, align 4
  store i1 true, ptr %resource.live.13, align 1
  %tmp.113 = load i1, ptr %package.live.6.2, align 1
  br i1 %tmp.113, label %package.field.drop.53, label %package.field.done.53
package.field.drop.53:
  store i1 false, ptr %package.live.6.2, align 1
  %tmp.114 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.114)
  br label %package.field.done.53
package.field.done.53:
  %v.95 = load %zil_env_0, ptr %local.13, align 4
  store i1 false, ptr %resource.live.13, align 1
  %tmp.115 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 2
  store %zil_env_0 %v.95, ptr %tmp.115, align 4
  store i1 true, ptr %package.live.6.2, align 1
  %tmp.116 = load i1, ptr %resource.live.13, align 1
  br i1 %tmp.116, label %closure.drop.54, label %closure.done.54
closure.drop.54:
  store i1 false, ptr %resource.live.13, align 1
  call void @zil.closure.drop.0(ptr %local.13)
  br label %closure.done.54
closure.done.54:
  %v.96 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.96, ptr %local.14, align 8
  store i1 true, ptr %resource.live.14, align 1
  %v.97 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.14, align 8
  store i1 false, ptr %resource.live.14, align 1
  %v.98 = call { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } @zil.fn.15({ { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.97)
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.98, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  store i1 true, ptr %package.live.6.0, align 1
  store i1 true, ptr %package.live.6.1, align 1
  store i1 true, ptr %package.live.6.2, align 1
  %tmp.117 = load i1, ptr %resource.live.14, align 1
  br i1 %tmp.117, label %package.drop.55, label %package.done.55
package.drop.55:
  store i1 false, ptr %resource.live.14, align 1
  %tmp.118 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.14, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.118)
  %tmp.119 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.14, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.119)
  %tmp.120 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.14, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.120)
  br label %package.done.55
package.done.55:
  %tmp.121 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 3
  %v.99 = load i32, ptr %tmp.121, align 4
  %v.100 = add i32 0, 1
  %v.101 = icmp ne i32 %v.99, %v.100
  br i1 %v.101, label %bb.31, label %bb.32
bb.31:
  %v.102 = add i32 0, 7
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.13, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.8, align 1
  %tmp.122 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.122, label %package.drop.56, label %package.done.56
package.drop.56:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.123 = load i1, ptr %package.live.6.0, align 1
  br i1 %tmp.123, label %package.field.drop.57, label %package.field.done.57
package.field.drop.57:
  store i1 false, ptr %package.live.6.0, align 1
  %tmp.124 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.124)
  br label %package.field.done.57
package.field.done.57:
  %tmp.125 = load i1, ptr %package.live.6.1, align 1
  br i1 %tmp.125, label %package.field.drop.58, label %package.field.done.58
package.field.drop.58:
  store i1 false, ptr %package.live.6.1, align 1
  %tmp.126 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.126)
  br label %package.field.done.58
package.field.done.58:
  %tmp.127 = load i1, ptr %package.live.6.2, align 1
  br i1 %tmp.127, label %package.field.drop.59, label %package.field.done.59
package.field.drop.59:
  store i1 false, ptr %package.live.6.2, align 1
  %tmp.128 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.128)
  br label %package.field.done.59
package.field.done.59:
  br label %package.done.56
package.done.56:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.129 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.129, label %resource.drop.60, label %resource.done.60
resource.drop.60:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.60
resource.done.60:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.15, align 1
  store i1 false, ptr %resource.live.16, align 1
  ret i32 %v.102
bb.32:
  br label %bb.33
bb.33:
  %v.103 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.103, ptr %local.15, align 8
  store i1 true, ptr %resource.live.15, align 1
  %v.104 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.15, align 8
  store i1 false, ptr %resource.live.15, align 1
  %v.105 = call { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } @zil.fn.15({ { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.104)
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.105, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  store i1 true, ptr %package.live.6.0, align 1
  store i1 true, ptr %package.live.6.1, align 1
  store i1 true, ptr %package.live.6.2, align 1
  %tmp.130 = load i1, ptr %resource.live.15, align 1
  br i1 %tmp.130, label %package.drop.61, label %package.done.61
package.drop.61:
  store i1 false, ptr %resource.live.15, align 1
  %tmp.131 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.15, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.131)
  %tmp.132 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.15, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.132)
  %tmp.133 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.15, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.133)
  br label %package.done.61
package.done.61:
  %tmp.134 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 3
  %v.106 = load i32, ptr %tmp.134, align 4
  %v.107 = add i32 0, 2
  %v.108 = icmp ne i32 %v.106, %v.107
  br i1 %v.108, label %bb.34, label %bb.35
bb.34:
  %v.109 = add i32 0, 8
  store i1 false, ptr %resource.live.15, align 1
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.13, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.8, align 1
  %tmp.135 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.135, label %package.drop.62, label %package.done.62
package.drop.62:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.136 = load i1, ptr %package.live.6.0, align 1
  br i1 %tmp.136, label %package.field.drop.63, label %package.field.done.63
package.field.drop.63:
  store i1 false, ptr %package.live.6.0, align 1
  %tmp.137 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.137)
  br label %package.field.done.63
package.field.done.63:
  %tmp.138 = load i1, ptr %package.live.6.1, align 1
  br i1 %tmp.138, label %package.field.drop.64, label %package.field.done.64
package.field.drop.64:
  store i1 false, ptr %package.live.6.1, align 1
  %tmp.139 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.139)
  br label %package.field.done.64
package.field.done.64:
  %tmp.140 = load i1, ptr %package.live.6.2, align 1
  br i1 %tmp.140, label %package.field.drop.65, label %package.field.done.65
package.field.drop.65:
  store i1 false, ptr %package.live.6.2, align 1
  %tmp.141 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.141)
  br label %package.field.done.65
package.field.done.65:
  br label %package.done.62
package.done.62:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.142 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.142, label %resource.drop.66, label %resource.done.66
resource.drop.66:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.66
resource.done.66:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.16, align 1
  ret i32 %v.109
bb.35:
  br label %bb.36
bb.36:
  %v.110 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.110, ptr %local.16, align 8
  store i1 true, ptr %resource.live.16, align 1
  %v.111 = load i32, ptr %local.4, align 4
  %v.112 = add i32 0, 2
  %tmp.143 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.111, i32 %v.112)
  %v.113 = extractvalue { i32, i1 } %tmp.143, 0
  %tmp.144 = extractvalue { i32, i1 } %tmp.143, 1
  br i1 %tmp.144, label %zil.trap, label %checked.67
checked.67:
  %v.114 = sext i32 %v.113 to i64
  %v.115 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.16, align 8
  store i1 false, ptr %resource.live.16, align 1
  %v.116 = call { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } @zil.fn.17({ { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.115, i64 %v.114)
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.116, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  store i1 true, ptr %package.live.6.0, align 1
  store i1 true, ptr %package.live.6.1, align 1
  store i1 true, ptr %package.live.6.2, align 1
  %tmp.145 = load i1, ptr %resource.live.16, align 1
  br i1 %tmp.145, label %package.drop.68, label %package.done.68
package.drop.68:
  store i1 false, ptr %resource.live.16, align 1
  %tmp.146 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.16, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.146)
  %tmp.147 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.16, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.147)
  %tmp.148 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.16, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.148)
  br label %package.done.68
package.done.68:
  %tmp.149 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 5
  %v.117 = load i1, ptr %tmp.149, align 1
  %v.118 = xor i1 %v.117, true
  br i1 %v.118, label %bb.37, label %bb.38
bb.37:
  %v.119 = add i32 0, 10
  store i1 false, ptr %resource.live.16, align 1
  store i1 false, ptr %resource.live.15, align 1
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.13, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.8, align 1
  %tmp.150 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.150, label %package.drop.69, label %package.done.69
package.drop.69:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.151 = load i1, ptr %package.live.6.0, align 1
  br i1 %tmp.151, label %package.field.drop.70, label %package.field.done.70
package.field.drop.70:
  store i1 false, ptr %package.live.6.0, align 1
  %tmp.152 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.152)
  br label %package.field.done.70
package.field.done.70:
  %tmp.153 = load i1, ptr %package.live.6.1, align 1
  br i1 %tmp.153, label %package.field.drop.71, label %package.field.done.71
package.field.drop.71:
  store i1 false, ptr %package.live.6.1, align 1
  %tmp.154 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.154)
  br label %package.field.done.71
package.field.done.71:
  %tmp.155 = load i1, ptr %package.live.6.2, align 1
  br i1 %tmp.155, label %package.field.drop.72, label %package.field.done.72
package.field.drop.72:
  store i1 false, ptr %package.live.6.2, align 1
  %tmp.156 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.156)
  br label %package.field.done.72
package.field.done.72:
  br label %package.done.69
package.done.69:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.157 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.157, label %resource.drop.73, label %resource.done.73
resource.drop.73:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.73
resource.done.73:
  store i1 false, ptr %resource.live.3, align 1
  ret i32 %v.119
bb.38:
  br label %bb.39
bb.39:
  %v.120 = add i32 0, 0
  store i1 false, ptr %resource.live.16, align 1
  store i1 false, ptr %resource.live.15, align 1
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.13, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.8, align 1
  %tmp.158 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.158, label %package.drop.74, label %package.done.74
package.drop.74:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.159 = load i1, ptr %package.live.6.0, align 1
  br i1 %tmp.159, label %package.field.drop.75, label %package.field.done.75
package.field.drop.75:
  store i1 false, ptr %package.live.6.0, align 1
  %tmp.160 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.160)
  br label %package.field.done.75
package.field.done.75:
  %tmp.161 = load i1, ptr %package.live.6.1, align 1
  br i1 %tmp.161, label %package.field.drop.76, label %package.field.done.76
package.field.drop.76:
  store i1 false, ptr %package.live.6.1, align 1
  %tmp.162 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.162)
  br label %package.field.done.76
package.field.done.76:
  %tmp.163 = load i1, ptr %package.live.6.2, align 1
  br i1 %tmp.163, label %package.field.drop.77, label %package.field.done.77
package.field.drop.77:
  store i1 false, ptr %package.live.6.2, align 1
  %tmp.164 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.6, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.164)
  br label %package.field.done.77
package.field.done.77:
  br label %package.done.74
package.done.74:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %package.live.6.0, align 1
  store i1 false, ptr %package.live.6.1, align 1
  store i1 false, ptr %package.live.6.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.165 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.165, label %resource.drop.78, label %resource.done.78
resource.drop.78:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.78
resource.done.78:
  store i1 false, ptr %resource.live.3, align 1
  ret i32 %v.120
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.3(ptr %arg.0) {
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

define { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.4(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { ptr, i64, i64 }, align 8
  %local.4 = alloca i32, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i32 }, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { ptr, i64, i64 }, align 8
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.5, align 1
  %v.0 = load i32, ptr %local.0, align 4
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
  %v.4 = load i32, ptr %local.0, align 4
  store i32 %v.4, ptr %local.4, align 4
  %v.5 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.5, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.6 = load { ptr, i64, i64 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  store { ptr, i64, i64 } %v.6, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  %v.7 = load i32, ptr %local.4, align 4
  %v.8 = load { i32 }, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
  %v.9 = load { ptr, i64, i64 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
%tmp.0 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } zeroinitializer, { i32 } %v.8, 0
%tmp.1 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } %tmp.0, { ptr, i64, i64 } %v.9, 1
%v.10 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } %tmp.1, i32 %v.7, 2
  %tmp.2 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.2, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.bytes.drop(ptr %local.6)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.3 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.3(ptr %local.5)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.4 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.4, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.bytes.drop(ptr %local.3)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.5 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.5, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.3(ptr %local.2)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.2, align 1
  ret { { i32 }, { ptr, i64, i64 }, i32 } %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load i32, ptr %v.2, align 4
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { ptr, i64, i64 }, i1, i32 } @zil.fn.6(i32 %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { ptr, i64, i64 }, align 8
  %local.3 = alloca i1, align 1
  %local.4 = alloca i32, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { ptr, i64, i64 }, align 8
  %local.6 = alloca i1, align 1
  %local.7 = alloca i32, align 4
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { ptr, i64, i64 }, align 8
  store i32 %arg.0, ptr %local.0, align 4
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = call { ptr, i64, i64 } @zil.fn.11()
  store i1 false, ptr %resource.live.2, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.1 = getelementptr i8, ptr %local.2, i64 0
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = load i32, ptr %local.1, align 4
  %v.4 = call i1 @zil.fn.8(ptr %v.1, i32 %v.2, i32 %v.3)
  store i1 %v.4, ptr %local.3, align 1
  %v.5 = getelementptr i8, ptr %local.2, i64 0
  %v.6 = call i32 @zil.fn.10(ptr %v.5)
  store i32 %v.6, ptr %local.4, align 4
  store i1 false, ptr %resource.live.8, align 1
  %v.7 = load { ptr, i64, i64 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  store { ptr, i64, i64 } %v.7, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.8 = load i1, ptr %local.3, align 1
  store i1 %v.8, ptr %local.6, align 1
  %v.9 = load i32, ptr %local.4, align 4
  store i32 %v.9, ptr %local.7, align 4
  %v.10 = load { ptr, i64, i64 }, ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
  store { ptr, i64, i64 } %v.10, ptr %local.8, align 8
  store i1 true, ptr %resource.live.8, align 1
  %v.11 = load i1, ptr %local.6, align 1
  %v.12 = load i32, ptr %local.7, align 4
  %v.13 = load { ptr, i64, i64 }, ptr %local.8, align 8
  store i1 false, ptr %resource.live.8, align 1
%tmp.0 = insertvalue { { ptr, i64, i64 }, i1, i32 } zeroinitializer, { ptr, i64, i64 } %v.13, 0
%tmp.1 = insertvalue { { ptr, i64, i64 }, i1, i32 } %tmp.0, i1 %v.11, 1
%v.14 = insertvalue { { ptr, i64, i64 }, i1, i32 } %tmp.1, i32 %v.12, 2
  %tmp.2 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.2, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.store.3.drop(ptr %local.8)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.3 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.store.3.drop(ptr %local.5)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.4 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.4, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.store.3.drop(ptr %local.2)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.2, align 1
  ret { { ptr, i64, i64 }, i1, i32 } %v.14
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.7(i32 %arg.0) {
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
  %v.3 = add i32 0, 0
  store i32 %v.3, ptr %local.3, align 4
  %v.4 = load i32, ptr %local.3, align 4
  %v.5 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.6 = load i32, ptr %local.0, align 4
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, i32 %v.4, 0
  %tmp.1 = insertvalue %zil_env_0 %tmp.0, { i32 } %v.5, 1
  %tmp.2 = insertvalue %zil_env_0 %tmp.1, i32 %v.6, 2
  %v.7 = insertvalue %zil_env_0 %tmp.2, i1 true, 3
  %tmp.3 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.3, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.3(ptr %local.2)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.2, align 1
  ret %zil_env_0 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i1 @zil.fn.8(ptr %arg.0, i32 %arg.1, i32 %arg.2) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  store i32 %arg.2, ptr %local.2, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = sext i32 %v.1 to i64
  %v.3 = call i1 @zil.store.3.reserve(ptr %v.0, i64 %v.2)
  %v.4 = xor i1 %v.3, true
  br i1 %v.4, label %bb.1, label %bb.2
bb.1:
  %v.5 = or i1 false, false
  ret i1 %v.5
bb.2:
  br label %bb.3
bb.3:
  %v.6 = add i32 0, 0
  store i32 %v.6, ptr %local.3, align 4
  br label %bb.4
bb.4:
  %v.7 = load i32, ptr %local.3, align 4
  %v.8 = load i32, ptr %local.1, align 4
  %v.9 = icmp slt i32 %v.7, %v.8
  br i1 %v.9, label %bb.5, label %bb.6
bb.5:
  store i1 false, ptr %resource.live.4, align 1
  %v.10 = load ptr, ptr %local.0, align 8
  %v.11 = load i32, ptr %local.2, align 4
  %v.12 = load i32, ptr %local.3, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.11, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.14 = call { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.4(i32 %v.13)
  %v.15 = call { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } @zil.store.3.push(ptr %v.10, { { i32 }, { ptr, i64, i64 }, i32 } %v.14)
  store { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } %v.15, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %tmp.2 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.2, label %error.drop.1, label %error.done.1
error.drop.1:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.3 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} }, ptr %local.4, i32 0, i32 0
  %tmp.4 = load i1, ptr %tmp.3, align 1
  br i1 %tmp.4, label %error.payload.1, label %success.payload.1
error.payload.1:
  %tmp.5 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} }, ptr %local.4, i32 0, i32 1
  %tmp.6 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.5, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.6)
  %tmp.7 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.5, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.7)
  br label %error.done.1
success.payload.1:
  br label %error.done.1
error.done.1:
  %v.16 = load i32, ptr %local.3, align 4
  %v.17 = add i32 0, 1
  %tmp.8 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.16, i32 %v.17)
  %v.18 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  store i32 %v.18, ptr %local.3, align 4
  store i1 false, ptr %resource.live.4, align 1
  br label %bb.4
bb.6:
  %v.19 = load ptr, ptr %local.0, align 8
  %tmp.10 = getelementptr { ptr, i64, i64 }, ptr %v.19, i32 0, i32 1
  %v.20 = load i64, ptr %tmp.10, align 8
  %v.21 = load i32, ptr %local.1, align 4
  %v.22 = sext i32 %v.21 to i64
  %v.23 = icmp eq i64 %v.20, %v.22
  ret i1 %v.23
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.9(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %v.0, i32 0, i32 2
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load i32, ptr %v.2, align 4
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.10(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  %local.2 = alloca i32, align 4
  %local.3 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = add i64 0, 0
  store i64 %v.0, ptr %local.1, align 8
  %v.1 = add i32 0, 0
  store i32 %v.1, ptr %local.2, align 4
  br label %bb.1
bb.1:
  %v.2 = load i64, ptr %local.1, align 8
  %v.3 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.3, i32 0, i32 1
  %v.4 = load i64, ptr %tmp.0, align 8
  %v.5 = icmp slt i64 %v.2, %v.4
  br i1 %v.5, label %bb.2, label %bb.3
bb.2:
  %v.6 = load ptr, ptr %local.0, align 8
  %v.7 = load i64, ptr %local.1, align 8
  %tmp.1 = getelementptr { ptr, i64, i64 }, ptr %v.6, i32 0, i32 1
  %tmp.2 = load i64, ptr %tmp.1, align 8
  %tmp.3 = icmp ult i64 %v.7, %tmp.2
  br i1 %tmp.3, label %store.element.ok.0, label %zil.trap
store.element.ok.0:
  %tmp.4 = getelementptr { ptr, i64, i64 }, ptr %v.6, i32 0, i32 0
  %tmp.5 = load ptr, ptr %tmp.4, align 8
  %v.8 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.5, i64 %v.7
  store ptr %v.8, ptr %local.3, align 8
  %v.9 = load i32, ptr %local.2, align 4
  %v.10 = load ptr, ptr %local.3, align 8
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = call i32 @zil.fn.9(ptr %v.11)
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.9, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.13, ptr %local.2, align 4
  %v.14 = load i64, ptr %local.1, align 8
  %v.15 = add i64 0, 1
  %tmp.8 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.14, i64 %v.15)
  %v.16 = extractvalue { i64, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i64, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  store i64 %v.16, ptr %local.1, align 8
  br label %bb.1
bb.3:
  %v.17 = load i32, ptr %local.2, align 4
  ret i32 %v.17
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.11() {
entry:
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i1 @zil.fn.12(ptr %arg.0, i64 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  %local.2 = alloca i64, align 8
  %local.3 = alloca i64, align 8
  %local.4 = alloca i32, align 4
  %local.5 = alloca i1, align 1
  %local.6 = alloca i1, align 1
  store ptr %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.0, i32 0, i32 1
  %v.1 = load i64, ptr %tmp.0, align 8
  store i64 %v.1, ptr %local.2, align 8
  %v.2 = load ptr, ptr %local.0, align 8
  %tmp.1 = getelementptr { ptr, i64, i64 }, ptr %v.2, i32 0, i32 2
  %v.3 = load i64, ptr %tmp.1, align 8
  store i64 %v.3, ptr %local.3, align 8
  %v.4 = load ptr, ptr %local.0, align 8
  %v.5 = call i32 @zil.fn.10(ptr %v.4)
  store i32 %v.5, ptr %local.4, align 4
  %v.6 = load ptr, ptr %local.0, align 8
  %v.7 = load i64, ptr %local.1, align 8
  %v.8 = call i1 @zil.store.3.reserve(ptr %v.6, i64 %v.7)
  store i1 %v.8, ptr %local.5, align 1
  %v.9 = load ptr, ptr %local.0, align 8
  %tmp.2 = getelementptr { ptr, i64, i64 }, ptr %v.9, i32 0, i32 1
  %v.10 = load i64, ptr %tmp.2, align 8
  %v.11 = load i64, ptr %local.2, align 8
  %v.12 = icmp ne i64 %v.10, %v.11
  br i1 %v.12, label %bb.1, label %bb.2
bb.1:
  %v.13 = or i1 false, false
  ret i1 %v.13
bb.2:
  br label %bb.3
bb.3:
  %v.14 = load i1, ptr %local.5, align 1
  %v.15 = xor i1 %v.14, true
  br i1 %v.15, label %bb.5, label %bb.4
bb.4:
  %v.16 = or i1 false, false
  store i1 %v.16, ptr %local.6, align 1
  br label %bb.6
bb.5:
  %v.17 = load ptr, ptr %local.0, align 8
  %tmp.3 = getelementptr { ptr, i64, i64 }, ptr %v.17, i32 0, i32 2
  %v.18 = load i64, ptr %tmp.3, align 8
  %v.19 = load i64, ptr %local.3, align 8
  %v.20 = icmp ne i64 %v.18, %v.19
  store i1 %v.20, ptr %local.6, align 1
  br label %bb.6
bb.6:
  %v.21 = load i1, ptr %local.6, align 1
  br i1 %v.21, label %bb.7, label %bb.8
bb.7:
  %v.22 = or i1 false, false
  ret i1 %v.22
bb.8:
  br label %bb.9
bb.9:
  %v.23 = load ptr, ptr %local.0, align 8
  %v.24 = call i32 @zil.fn.10(ptr %v.23)
  %v.25 = load i32, ptr %local.4, align 4
  %v.26 = icmp eq i32 %v.24, %v.25
  ret i1 %v.26
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.13(ptr %env) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca ptr, align 8
  %local.2 = alloca i32, align 4
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store ptr %v.1, ptr %local.1, align 8
  %tmp.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
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
  %tmp.4 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
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

define { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } @zil.fn.14(%zil_env_0 %arg.0, i32 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { ptr, i64, i64 }, i1, i32 }, align 8
  %package.live.2.0 = alloca i1, align 1
  store i1 false, ptr %package.live.2.0, align 1
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i32 }, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { ptr, i64, i64 }, align 8
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca %zil_env_0, align 4
  %local.7 = alloca i32, align 4
  %local.8 = alloca i32, align 4
  %local.9 = alloca i1, align 1
  %resource.live.10 = alloca i1, align 1
  store i1 false, ptr %resource.live.10, align 1
  %local.10 = alloca { i32 }, align 4
  %resource.live.11 = alloca i1, align 1
  store i1 false, ptr %resource.live.11, align 1
  %local.11 = alloca { ptr, i64, i64 }, align 8
  %resource.live.12 = alloca i1, align 1
  store i1 false, ptr %resource.live.12, align 1
  %local.12 = alloca %zil_env_0, align 4
  store %zil_env_0 %arg.0, ptr %local.0, align 4
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.1, align 4
  %v.1 = add i32 0, 68
  %v.2 = call { { ptr, i64, i64 }, i1, i32 } @zil.fn.6(i32 %v.0, i32 %v.1)
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store { { ptr, i64, i64 }, i1, i32 } %v.2, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  store i1 true, ptr %package.live.2.0, align 1
  store i1 false, ptr %resource.live.12, align 1
  store i1 false, ptr %resource.live.11, align 1
  store i1 false, ptr %resource.live.10, align 1
  %v.3 = add i32 0, 90
  store i32 %v.3, ptr %local.3, align 4
  %v.4 = load i32, ptr %local.3, align 4
%v.5 = insertvalue { i32 } zeroinitializer, i32 %v.4, 0
  store i1 false, ptr %resource.live.4, align 1
  store { i32 } %v.5, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %tmp.0 = getelementptr { { ptr, i64, i64 }, i1, i32 }, ptr %local.2, i32 0, i32 0
  %v.6 = load { ptr, i64, i64 }, ptr %tmp.0, align 8
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %resource.live.5, align 1
  store { ptr, i64, i64 } %v.6, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.7 = load %zil_env_0, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.6, align 1
  store %zil_env_0 %v.7, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.8 = add i32 0, 0
  store i32 %v.8, ptr %local.7, align 4
  %tmp.1 = getelementptr { { ptr, i64, i64 }, i1, i32 }, ptr %local.2, i32 0, i32 2
  %v.9 = load i32, ptr %tmp.1, align 4
  store i32 %v.9, ptr %local.8, align 4
  %tmp.2 = getelementptr { { ptr, i64, i64 }, i1, i32 }, ptr %local.2, i32 0, i32 1
  %v.10 = load i1, ptr %tmp.2, align 1
  store i1 %v.10, ptr %local.9, align 1
  %v.11 = load { i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  store { i32 } %v.11, ptr %local.10, align 4
  store i1 true, ptr %resource.live.10, align 1
  %v.12 = load { ptr, i64, i64 }, ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
  store { ptr, i64, i64 } %v.12, ptr %local.11, align 8
  store i1 true, ptr %resource.live.11, align 1
  %v.13 = load %zil_env_0, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  store %zil_env_0 %v.13, ptr %local.12, align 4
  store i1 true, ptr %resource.live.12, align 1
  %v.14 = load i32, ptr %local.7, align 4
  %v.15 = load i32, ptr %local.8, align 4
  %v.16 = load i1, ptr %local.9, align 1
  %v.17 = load { i32 }, ptr %local.10, align 4
  store i1 false, ptr %resource.live.10, align 1
  %v.18 = load { ptr, i64, i64 }, ptr %local.11, align 8
  store i1 false, ptr %resource.live.11, align 1
  %v.19 = load %zil_env_0, ptr %local.12, align 4
  store i1 false, ptr %resource.live.12, align 1
%tmp.3 = insertvalue { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } zeroinitializer, { i32 } %v.17, 0
%tmp.4 = insertvalue { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %tmp.3, { ptr, i64, i64 } %v.18, 1
%tmp.5 = insertvalue { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %tmp.4, %zil_env_0 %v.19, 2
%tmp.6 = insertvalue { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %tmp.5, i32 %v.14, 3
%tmp.7 = insertvalue { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %tmp.6, i32 %v.15, 4
%v.20 = insertvalue { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %tmp.7, i1 %v.16, 5
  %tmp.8 = load i1, ptr %resource.live.12, align 1
  br i1 %tmp.8, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.12, align 1
  call void @zil.closure.drop.0(ptr %local.12)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.12, align 1
  %tmp.9 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.9, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.11, align 1
  call void @zil.store.3.drop(ptr %local.11)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.11, align 1
  %tmp.10 = load i1, ptr %resource.live.10, align 1
  br i1 %tmp.10, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.10, align 1
  call void @zil.fn.3(ptr %local.10)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.10, align 1
  %tmp.11 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.11, label %closure.drop.3, label %closure.done.3
closure.drop.3:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.closure.drop.0(ptr %local.6)
  br label %closure.done.3
closure.done.3:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.12 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.12, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.store.3.drop(ptr %local.5)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.13 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.13, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.fn.3(ptr %local.4)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.14 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.14, label %package.drop.6, label %package.done.6
package.drop.6:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.15 = load i1, ptr %package.live.2.0, align 1
  br i1 %tmp.15, label %package.field.drop.7, label %package.field.done.7
package.field.drop.7:
  store i1 false, ptr %package.live.2.0, align 1
  %tmp.16 = getelementptr { { ptr, i64, i64 }, i1, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.store.3.drop(ptr %tmp.16)
  br label %package.field.done.7
package.field.done.7:
  br label %package.done.6
package.done.6:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  %tmp.17 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.17, label %closure.drop.8, label %closure.done.8
closure.drop.8:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.8
closure.done.8:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.20
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } @zil.fn.15({ { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  %package.live.1.0 = alloca i1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  %package.live.1.1 = alloca i1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %package.live.1.2 = alloca i1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  %local.2 = alloca ptr, align 8
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.0, align 1
  store i1 true, ptr %package.live.1.1, align 1
  store i1 true, ptr %package.live.1.2, align 1
  %v.1 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 2
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  store ptr %v.2, ptr %local.2, align 8
  %v.3 = load ptr, ptr %local.2, align 8
  %v.4 = call i32 @zil.fn.13(ptr %v.3)
  %tmp.0 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 3
  store i32 %v.4, ptr %tmp.0, align 4
  %v.5 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  %tmp.1 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.1, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.2 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.2, label %package.field.drop.1, label %package.field.done.1
package.field.drop.1:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.3 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.3)
  br label %package.field.done.1
package.field.done.1:
  %tmp.4 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.4, label %package.field.drop.2, label %package.field.done.2
package.field.drop.2:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.5 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.5)
  br label %package.field.done.2
package.field.done.2:
  %tmp.6 = load i1, ptr %package.live.1.2, align 1
  br i1 %tmp.6, label %package.field.drop.3, label %package.field.done.3
package.field.drop.3:
  store i1 false, ptr %package.live.1.2, align 1
  %tmp.7 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.7)
  br label %package.field.done.3
package.field.done.3:
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  %tmp.8 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.8, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.9 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.9)
  %tmp.10 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.10)
  %tmp.11 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.11)
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } @zil.fn.16({ { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %arg.0, i32 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  %package.live.2.0 = alloca i1, align 1
  store i1 false, ptr %package.live.2.0, align 1
  %package.live.2.1 = alloca i1, align 1
  store i1 false, ptr %package.live.2.1, align 1
  %package.live.2.2 = alloca i1, align 1
  store i1 false, ptr %package.live.2.2, align 1
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { ptr, i64, i64 }, i1, i32 }, align 8
  %package.live.3.0 = alloca i1, align 1
  store i1 false, ptr %package.live.3.0, align 1
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { ptr, i64, i64 }, align 8
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.4, align 1
  %v.0 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  store i1 false, ptr %package.live.2.2, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.0, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  store i1 true, ptr %package.live.2.0, align 1
  store i1 true, ptr %package.live.2.1, align 1
  store i1 true, ptr %package.live.2.2, align 1
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = add i32 0, 77
  %v.3 = call { { ptr, i64, i64 }, i1, i32 } @zil.fn.6(i32 %v.1, i32 %v.2)
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %package.live.3.0, align 1
  store { { ptr, i64, i64 }, i1, i32 } %v.3, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  store i1 true, ptr %package.live.3.0, align 1
  %tmp.0 = getelementptr { { ptr, i64, i64 }, i1, i32 }, ptr %local.3, i32 0, i32 1
  %v.4 = load i1, ptr %tmp.0, align 1
  %v.5 = xor i1 %v.4, true
  br i1 %v.5, label %bb.1, label %bb.2
bb.1:
  %v.6 = or i1 false, false
  %tmp.1 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 5
  store i1 %v.6, ptr %tmp.1, align 1
  %v.7 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 1
  %v.8 = call i32 @zil.fn.10(ptr %v.7)
  %tmp.2 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 4
  store i32 %v.8, ptr %tmp.2, align 4
  %v.9 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  store i1 false, ptr %package.live.2.2, align 1
  %tmp.3 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.3, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.4 = load i1, ptr %package.live.3.0, align 1
  br i1 %tmp.4, label %package.field.drop.1, label %package.field.done.1
package.field.drop.1:
  store i1 false, ptr %package.live.3.0, align 1
  %tmp.5 = getelementptr { { ptr, i64, i64 }, i1, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.store.3.drop(ptr %tmp.5)
  br label %package.field.done.1
package.field.done.1:
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %package.live.3.0, align 1
  %tmp.6 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.6, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.7 = load i1, ptr %package.live.2.0, align 1
  br i1 %tmp.7, label %package.field.drop.3, label %package.field.done.3
package.field.drop.3:
  store i1 false, ptr %package.live.2.0, align 1
  %tmp.8 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.8)
  br label %package.field.done.3
package.field.done.3:
  %tmp.9 = load i1, ptr %package.live.2.1, align 1
  br i1 %tmp.9, label %package.field.drop.4, label %package.field.done.4
package.field.drop.4:
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.10 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.10)
  br label %package.field.done.4
package.field.done.4:
  %tmp.11 = load i1, ptr %package.live.2.2, align 1
  br i1 %tmp.11, label %package.field.drop.5, label %package.field.done.5
package.field.drop.5:
  store i1 false, ptr %package.live.2.2, align 1
  %tmp.12 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.12)
  br label %package.field.done.5
package.field.done.5:
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  store i1 false, ptr %package.live.2.2, align 1
  store i1 false, ptr %resource.live.4, align 1
  %tmp.13 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.13, label %package.drop.6, label %package.done.6
package.drop.6:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.14 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.14)
  %tmp.15 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.15)
  %tmp.16 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.16)
  br label %package.done.6
package.done.6:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.9
bb.2:
  br label %bb.3
bb.3:
  %tmp.17 = getelementptr { { ptr, i64, i64 }, i1, i32 }, ptr %local.3, i32 0, i32 0
  %v.10 = load { ptr, i64, i64 }, ptr %tmp.17, align 8
  store i1 false, ptr %package.live.3.0, align 1
  store { ptr, i64, i64 } %v.10, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %tmp.18 = load i1, ptr %package.live.2.1, align 1
  br i1 %tmp.18, label %package.field.drop.7, label %package.field.done.7
package.field.drop.7:
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.19 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.19)
  br label %package.field.done.7
package.field.done.7:
  %v.11 = load { ptr, i64, i64 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  %tmp.20 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 1
  store { ptr, i64, i64 } %v.11, ptr %tmp.20, align 8
  store i1 true, ptr %package.live.2.1, align 1
  %tmp.21 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.21, label %resource.drop.8, label %resource.done.8
resource.drop.8:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.store.3.drop(ptr %local.4)
  br label %resource.done.8
resource.done.8:
  %v.12 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 1
  %v.13 = call i32 @zil.fn.10(ptr %v.12)
  %tmp.22 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 4
  store i32 %v.13, ptr %tmp.22, align 4
  %v.14 = or i1 false, true
  %tmp.23 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 5
  store i1 %v.14, ptr %tmp.23, align 1
  %v.15 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  store i1 false, ptr %package.live.2.2, align 1
  store i1 false, ptr %resource.live.4, align 1
  %tmp.24 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.24, label %package.drop.9, label %package.done.9
package.drop.9:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.25 = load i1, ptr %package.live.3.0, align 1
  br i1 %tmp.25, label %package.field.drop.10, label %package.field.done.10
package.field.drop.10:
  store i1 false, ptr %package.live.3.0, align 1
  %tmp.26 = getelementptr { { ptr, i64, i64 }, i1, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.store.3.drop(ptr %tmp.26)
  br label %package.field.done.10
package.field.done.10:
  br label %package.done.9
package.done.9:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %package.live.3.0, align 1
  %tmp.27 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.27, label %package.drop.11, label %package.done.11
package.drop.11:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.28 = load i1, ptr %package.live.2.0, align 1
  br i1 %tmp.28, label %package.field.drop.12, label %package.field.done.12
package.field.drop.12:
  store i1 false, ptr %package.live.2.0, align 1
  %tmp.29 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.29)
  br label %package.field.done.12
package.field.done.12:
  %tmp.30 = load i1, ptr %package.live.2.1, align 1
  br i1 %tmp.30, label %package.field.drop.13, label %package.field.done.13
package.field.drop.13:
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.31 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.31)
  br label %package.field.done.13
package.field.done.13:
  %tmp.32 = load i1, ptr %package.live.2.2, align 1
  br i1 %tmp.32, label %package.field.drop.14, label %package.field.done.14
package.field.drop.14:
  store i1 false, ptr %package.live.2.2, align 1
  %tmp.33 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.33)
  br label %package.field.done.14
package.field.done.14:
  br label %package.done.11
package.done.11:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  store i1 false, ptr %package.live.2.2, align 1
  %tmp.34 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.34, label %package.drop.15, label %package.done.15
package.drop.15:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.35 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.35)
  %tmp.36 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.36)
  %tmp.37 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.37)
  br label %package.done.15
package.done.15:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.15
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } @zil.fn.17({ { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %arg.0, i64 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  %local.1 = alloca i64, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  %package.live.2.0 = alloca i1, align 1
  store i1 false, ptr %package.live.2.0, align 1
  %package.live.2.1 = alloca i1, align 1
  store i1 false, ptr %package.live.2.1, align 1
  %package.live.2.2 = alloca i1, align 1
  store i1 false, ptr %package.live.2.2, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  store i1 false, ptr %package.live.2.2, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.0, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  store i1 true, ptr %package.live.2.0, align 1
  store i1 true, ptr %package.live.2.1, align 1
  store i1 true, ptr %package.live.2.2, align 1
  %v.1 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 1
  %v.2 = load i64, ptr %local.1, align 8
  %v.3 = call i1 @zil.fn.12(ptr %v.1, i64 %v.2)
  %tmp.0 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 5
  store i1 %v.3, ptr %tmp.0, align 1
  %v.4 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 1
  %v.5 = call i32 @zil.fn.10(ptr %v.4)
  %tmp.1 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 4
  store i32 %v.5, ptr %tmp.1, align 4
  %v.6 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  store i1 false, ptr %package.live.2.2, align 1
  %tmp.2 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.2, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.3 = load i1, ptr %package.live.2.0, align 1
  br i1 %tmp.3, label %package.field.drop.1, label %package.field.done.1
package.field.drop.1:
  store i1 false, ptr %package.live.2.0, align 1
  %tmp.4 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.4)
  br label %package.field.done.1
package.field.done.1:
  %tmp.5 = load i1, ptr %package.live.2.1, align 1
  br i1 %tmp.5, label %package.field.drop.2, label %package.field.done.2
package.field.drop.2:
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.6 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.6)
  br label %package.field.done.2
package.field.done.2:
  %tmp.7 = load i1, ptr %package.live.2.2, align 1
  br i1 %tmp.7, label %package.field.drop.3, label %package.field.done.3
package.field.drop.3:
  store i1 false, ptr %package.live.2.2, align 1
  %tmp.8 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.2, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.8)
  br label %package.field.done.3
package.field.done.3:
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  store i1 false, ptr %package.live.2.2, align 1
  %tmp.9 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.9, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.10 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.10)
  %tmp.11 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.11)
  %tmp.12 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.12)
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}
