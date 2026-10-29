; ZIL verified LLVM module
; verified-fingerprint: ace0c63dfc2dc3d2be81f1b0dc485c39c90b41cc3c41011345058a210d6118fb
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
  call void @zil.fn.5(ptr %payload.1)
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

define internal i1 @zil.store.2.reserve(ptr %head, i64 %requested) {
entry:
  %stride = ptrtoint ptr getelementptr ({ { i32 }, i32 }, ptr null, i64 1) to i64
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
define internal { i1, { { i32 }, i32 }, {} } @zil.store.2.push(ptr %head, { { i32 }, i32 } %item) {
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
  %slot = getelementptr { { i32 }, i32 }, ptr %data, i64 %len
  store { { i32 }, i32 } %item, ptr %slot, align 4
  %next.len = add i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  ret { i1, { { i32 }, i32 }, {} } zeroinitializer
full:
  %tag = insertvalue { i1, { { i32 }, i32 }, {} } zeroinitializer, i1 true, 0
  %error = insertvalue { i1, { { i32 }, i32 }, {} } %tag, { { i32 }, i32 } %item, 1
  ret { i1, { { i32 }, i32 }, {} } %error
}
define internal { i1, {}, { { i32 }, i32 } } @zil.store.2.pop(ptr %head) {
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
  %slot = getelementptr { { i32 }, i32 }, ptr %data, i64 %next.len
  %item = load { { i32 }, i32 }, ptr %slot, align 4
  %result = insertvalue { i1, {}, { { i32 }, i32 } } zeroinitializer, { { i32 }, i32 } %item, 2
  ret { i1, {}, { { i32 }, i32 } } %result
empty:
  %error = insertvalue { i1, {}, { { i32 }, i32 } } zeroinitializer, i1 true, 0
  ret { i1, {}, { { i32 }, i32 } } %error
}
define internal void @zil.store.2.drop(ptr %head) {
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
  %slot = getelementptr { { i32 }, i32 }, ptr %data, i64 %index
  %field.0 = getelementptr { { i32 }, i32 }, ptr %slot, i32 0, i32 0
  call void @zil.fn.5(ptr %field.0)
  %next = add i64 %index, 1
  br label %loop
release:
  call void @zil.bytes.release(ptr %data)
  ret void
}
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.smul.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @getchar() nonlazybind
define i32 @zil.fn.1() {
entry:
  br label %bb.0
bb.0:
  %v.0 = call i32 @getchar()
  %v.1 = add i32 0, 48
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.0, i32 %v.1)
  %v.2 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { ptr, i64, i64 }, { ptr, i64, i64 } } @zil.fn.2() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { ptr, i64, i64 }, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { ptr, i64, i64 }, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { ptr, i64, i64 }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { ptr, i64, i64 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = call { ptr, i64, i64 } @zil.fn.9()
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.1, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.2 = load { ptr, i64, i64 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.2, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = load { ptr, i64, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.3, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.4 = load { ptr, i64, i64 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  %v.5 = load { ptr, i64, i64 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
%tmp.0 = insertvalue { { ptr, i64, i64 }, { ptr, i64, i64 } } zeroinitializer, { ptr, i64, i64 } %v.4, 0
%v.6 = insertvalue { { ptr, i64, i64 }, { ptr, i64, i64 } } %tmp.0, { ptr, i64, i64 } %v.5, 1
  %tmp.1 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.bytes.drop(ptr %local.3)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.2 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.store.2.drop(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.3 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.3, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.bytes.drop(ptr %local.1)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.4 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.4, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.2.drop(ptr %local.0)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.0, align 1
  ret { { ptr, i64, i64 }, { ptr, i64, i64 } } %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_0, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, align 8
  %package.live.3.0 = alloca i1, align 1
  store i1 false, ptr %package.live.3.0, align 1
  %package.live.3.1 = alloca i1, align 1
  store i1 false, ptr %package.live.3.1, align 1
  %package.live.3.3 = alloca i1, align 1
  store i1 false, ptr %package.live.3.3, align 1
  %local.4 = alloca ptr, align 8
  %local.5 = alloca i32, align 4
  %local.6 = alloca ptr, align 8
  %local.7 = alloca { ptr, i64 }, align 8
  %local.8 = alloca ptr, align 8
  %local.9 = alloca i32, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = call i32 @zil.fn.1()
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = add i32 0, 1
  %v.3 = icmp slt i32 %v.1, %v.2
  br i1 %v.3, label %bb.1, label %bb.2
bb.1:
  %v.4 = or i1 false, true
  store i1 %v.4, ptr %local.1, align 1
  br label %bb.3
bb.2:
  %v.5 = load i32, ptr %local.0, align 4
  %v.6 = add i32 0, 2
  %v.7 = icmp sgt i32 %v.5, %v.6
  store i1 %v.7, ptr %local.1, align 1
  br label %bb.3
bb.3:
  %v.8 = load i1, ptr %local.1, align 1
  br i1 %v.8, label %bb.4, label %bb.5
bb.4:
  %v.9 = add i32 0, 9
  store i1 false, ptr %resource.live.2, align 1
  ret i32 %v.9
bb.5:
  br label %bb.6
bb.6:
  %v.10 = add i32 0, 65
  %v.11 = call %zil_env_0 @zil.fn.8(i32 %v.10)
  store %zil_env_0 %v.11, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.12 = load %zil_env_0, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.13 = call { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } } @zil.fn.13(%zil_env_0 %v.12)
  %tmp.0 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.0, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %package.live.3.0, align 1
  store i1 false, ptr %package.live.3.1, align 1
  store i1 false, ptr %package.live.3.3, align 1
  store { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } } %v.13, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  store i1 true, ptr %package.live.3.0, align 1
  store i1 true, ptr %package.live.3.1, align 1
  store i1 true, ptr %package.live.3.3, align 1
  %v.14 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 1
  %v.15 = getelementptr i8, ptr %v.14, i64 0
  store ptr %v.15, ptr %local.4, align 8
  %v.16 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 0
  %v.17 = load i32, ptr %local.0, align 4
  %v.18 = sext i32 %v.17 to i64
  %v.19 = call i1 @zil.fn.10(ptr %v.16, i64 %v.18)
  %v.20 = xor i1 %v.19, true
  br i1 %v.20, label %bb.7, label %bb.8
bb.7:
  %v.21 = load ptr, ptr %local.4, align 8
  %v.22 = call i32 @zil.fn.12(ptr %v.21)
  %v.23 = add i32 0, 20
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.22, i32 %v.23)
  %v.24 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.1
checked.1:
  %tmp.3 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.3, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.4 = load i1, ptr %package.live.3.0, align 1
  br i1 %tmp.4, label %package.field.drop.3, label %package.field.done.3
package.field.drop.3:
  store i1 false, ptr %package.live.3.0, align 1
  %tmp.5 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.5)
  br label %package.field.done.3
package.field.done.3:
  %tmp.6 = load i1, ptr %package.live.3.1, align 1
  br i1 %tmp.6, label %package.field.drop.4, label %package.field.done.4
package.field.drop.4:
  store i1 false, ptr %package.live.3.1, align 1
  %tmp.7 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.7)
  br label %package.field.done.4
package.field.done.4:
  %tmp.8 = load i1, ptr %package.live.3.3, align 1
  br i1 %tmp.8, label %package.field.drop.5, label %package.field.done.5
package.field.drop.5:
  store i1 false, ptr %package.live.3.3, align 1
  %tmp.9 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 3
  call void @zil.bytes.drop(ptr %tmp.9)
  br label %package.field.done.5
package.field.done.5:
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %package.live.3.0, align 1
  store i1 false, ptr %package.live.3.1, align 1
  store i1 false, ptr %package.live.3.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  ret i32 %v.24
bb.8:
  br label %bb.9
bb.9:
  %v.25 = add i32 0, 0
  store i32 %v.25, ptr %local.5, align 4
  br label %bb.10
bb.10:
  %v.26 = load i32, ptr %local.5, align 4
  %v.27 = load i32, ptr %local.0, align 4
  %v.28 = icmp slt i32 %v.26, %v.27
  br i1 %v.28, label %bb.11, label %bb.12
bb.11:
  %v.29 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 0
  %v.30 = add i32 0, 66
  %v.31 = load i32, ptr %local.5, align 4
  %tmp.10 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.30, i32 %v.31)
  %v.32 = extractvalue { i32, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i32, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.6
checked.6:
  call void @zil.fn.11(ptr %v.29, i32 %v.32)
  %v.34 = load i32, ptr %local.5, align 4
  %v.35 = add i32 0, 1
  %tmp.12 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.34, i32 %v.35)
  %v.36 = extractvalue { i32, i1 } %tmp.12, 0
  %tmp.13 = extractvalue { i32, i1 } %tmp.12, 1
  br i1 %tmp.13, label %zil.trap, label %checked.7
checked.7:
  store i32 %v.36, ptr %local.5, align 4
  br label %bb.10
bb.12:
  %v.37 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 0
  %v.38 = load i32, ptr %local.0, align 4
  %v.39 = add i32 0, 1
  %tmp.14 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.38, i32 %v.39)
  %v.40 = extractvalue { i32, i1 } %tmp.14, 0
  %tmp.15 = extractvalue { i32, i1 } %tmp.14, 1
  br i1 %tmp.15, label %zil.trap, label %checked.8
checked.8:
  %v.41 = sext i32 %v.40 to i64
  %tmp.16 = getelementptr { ptr, i64, i64 }, ptr %v.37, i32 0, i32 1
  %tmp.17 = load i64, ptr %tmp.16, align 8
  %tmp.18 = icmp ult i64 %v.41, %tmp.17
  br i1 %tmp.18, label %store.element.ok.9, label %zil.trap
store.element.ok.9:
  %tmp.19 = getelementptr { ptr, i64, i64 }, ptr %v.37, i32 0, i32 0
  %tmp.20 = load ptr, ptr %tmp.19, align 8
  %v.42 = getelementptr { { i32 }, i32 }, ptr %tmp.20, i64 %v.41
  store ptr %v.42, ptr %local.6, align 8
  %v.43 = load ptr, ptr %local.6, align 8
  %v.44 = getelementptr { { i32 }, i32 }, ptr %v.43, i32 0, i32 1
  %v.45 = getelementptr i8, ptr %v.44, i64 0
  %v.46 = load i32, ptr %v.45, align 4
  %tmp.21 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 2
  store i32 %v.46, ptr %tmp.21, align 4
  %tmp.22 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 2
  %v.47 = load i32, ptr %tmp.22, align 4
  %v.48 = load ptr, ptr %local.4, align 8
  %v.49 = call i32 @zil.fn.12(ptr %v.48)
  %tmp.23 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.47, i32 %v.49)
  %v.50 = extractvalue { i32, i1 } %tmp.23, 0
  %tmp.24 = extractvalue { i32, i1 } %tmp.23, 1
  br i1 %tmp.24, label %zil.trap, label %checked.10
checked.10:
  %tmp.25 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 2
  store i32 %v.50, ptr %tmp.25, align 4
  %tmp.26 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 2
  %v.51 = load i32, ptr %tmp.26, align 4
  %v.52 = load ptr, ptr %local.4, align 8
  %v.53 = call i32 @zil.fn.12(ptr %v.52)
  %tmp.27 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.51, i32 %v.53)
  %v.54 = extractvalue { i32, i1 } %tmp.27, 0
  %tmp.28 = extractvalue { i32, i1 } %tmp.27, 1
  br i1 %tmp.28, label %zil.trap, label %checked.11
checked.11:
  %tmp.29 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 2
  store i32 %v.54, ptr %tmp.29, align 4
  %v.55 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 3
  %v.56 = add i64 0, 4
  %v.57 = call i1 @zil.bytes.reserve(ptr %v.55, i64 %v.56)
  %v.58 = xor i1 %v.57, true
  br i1 %v.58, label %bb.13, label %bb.14
bb.13:
  %v.59 = load ptr, ptr %local.4, align 8
  %v.60 = call i32 @zil.fn.12(ptr %v.59)
  %v.61 = add i32 0, 19
  %tmp.30 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.60, i32 %v.61)
  %v.62 = extractvalue { i32, i1 } %tmp.30, 0
  %tmp.31 = extractvalue { i32, i1 } %tmp.30, 1
  br i1 %tmp.31, label %zil.trap, label %checked.12
checked.12:
  %tmp.32 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.32, label %package.drop.13, label %package.done.13
package.drop.13:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.33 = load i1, ptr %package.live.3.0, align 1
  br i1 %tmp.33, label %package.field.drop.14, label %package.field.done.14
package.field.drop.14:
  store i1 false, ptr %package.live.3.0, align 1
  %tmp.34 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.34)
  br label %package.field.done.14
package.field.done.14:
  %tmp.35 = load i1, ptr %package.live.3.1, align 1
  br i1 %tmp.35, label %package.field.drop.15, label %package.field.done.15
package.field.drop.15:
  store i1 false, ptr %package.live.3.1, align 1
  %tmp.36 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.36)
  br label %package.field.done.15
package.field.done.15:
  %tmp.37 = load i1, ptr %package.live.3.3, align 1
  br i1 %tmp.37, label %package.field.drop.16, label %package.field.done.16
package.field.drop.16:
  store i1 false, ptr %package.live.3.3, align 1
  %tmp.38 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 3
  call void @zil.bytes.drop(ptr %tmp.38)
  br label %package.field.done.16
package.field.done.16:
  br label %package.done.13
package.done.13:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %package.live.3.0, align 1
  store i1 false, ptr %package.live.3.1, align 1
  store i1 false, ptr %package.live.3.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  ret i32 %v.62
bb.14:
  br label %bb.15
bb.15:
  %v.63 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 3
  %v.64 = add i8 0, 9
  %v.65 = call i1 @zil.bytes.push(ptr %v.63, i8 %v.64)
  %v.66 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 3
  %tmp.39 = load ptr, ptr %v.66, align 8
  %tmp.40 = getelementptr { ptr, i64, i64 }, ptr %v.66, i32 0, i32 1
  %tmp.41 = load i64, ptr %tmp.40, align 8
  %tmp.42 = insertvalue { ptr, i64 } poison, ptr %tmp.39, 0
  %v.67 = insertvalue { ptr, i64 } %tmp.42, i64 %tmp.41, 1
  store { ptr, i64 } %v.67, ptr %local.7, align 8
  %v.68 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 0
  %v.69 = load i32, ptr %local.0, align 4
  %v.70 = add i32 0, 1
  %tmp.43 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.69, i32 %v.70)
  %v.71 = extractvalue { i32, i1 } %tmp.43, 0
  %tmp.44 = extractvalue { i32, i1 } %tmp.43, 1
  br i1 %tmp.44, label %zil.trap, label %checked.17
checked.17:
  %v.72 = sext i32 %v.71 to i64
  %tmp.45 = getelementptr { ptr, i64, i64 }, ptr %v.68, i32 0, i32 1
  %tmp.46 = load i64, ptr %tmp.45, align 8
  %tmp.47 = icmp ult i64 %v.72, %tmp.46
  br i1 %tmp.47, label %store.element.ok.18, label %zil.trap
store.element.ok.18:
  %tmp.48 = getelementptr { ptr, i64, i64 }, ptr %v.68, i32 0, i32 0
  %tmp.49 = load ptr, ptr %tmp.48, align 8
  %v.73 = getelementptr { { i32 }, i32 }, ptr %tmp.49, i64 %v.72
  store ptr %v.73, ptr %local.8, align 8
  %tmp.50 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 2
  %v.74 = load i32, ptr %tmp.50, align 4
  %v.75 = load ptr, ptr %local.4, align 8
  %v.76 = call i32 @zil.fn.12(ptr %v.75)
  %tmp.51 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.74, i32 %v.76)
  %v.77 = extractvalue { i32, i1 } %tmp.51, 0
  %tmp.52 = extractvalue { i32, i1 } %tmp.51, 1
  br i1 %tmp.52, label %zil.trap, label %checked.19
checked.19:
  %tmp.53 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 2
  store i32 %v.77, ptr %tmp.53, align 4
  %tmp.54 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 2
  %v.78 = load i32, ptr %tmp.54, align 4
  %v.79 = load ptr, ptr %local.8, align 8
  %v.80 = getelementptr { { i32 }, i32 }, ptr %v.79, i32 0, i32 1
  %v.81 = getelementptr i8, ptr %v.80, i64 0
  %v.82 = load i32, ptr %v.81, align 4
  %tmp.55 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.78, i32 %v.82)
  %v.83 = extractvalue { i32, i1 } %tmp.55, 0
  %tmp.56 = extractvalue { i32, i1 } %tmp.55, 1
  br i1 %tmp.56, label %zil.trap, label %checked.20
checked.20:
  %v.84 = load { ptr, i64 }, ptr %local.7, align 8
  %v.85 = extractvalue { ptr, i64 } %v.84, 1
  %tmp.57 = icmp slt i64 %v.85, -2147483648
  %tmp.58 = icmp sgt i64 %v.85, 2147483647
  %tmp.59 = or i1 %tmp.57, %tmp.58
  br i1 %tmp.59, label %zil.trap, label %checked.21
checked.21:
  %v.86 = trunc i64 %v.85 to i32
  %tmp.60 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.83, i32 %v.86)
  %v.87 = extractvalue { i32, i1 } %tmp.60, 0
  %tmp.61 = extractvalue { i32, i1 } %tmp.60, 1
  br i1 %tmp.61, label %zil.trap, label %checked.22
checked.22:
  store i32 %v.87, ptr %local.9, align 4
  %v.88 = load i32, ptr %local.9, align 4
  %v.89 = add i32 0, 2
  %v.90 = add i32 0, 65
  %v.91 = load i32, ptr %local.0, align 4
  %tmp.62 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.90, i32 %v.91)
  %v.92 = extractvalue { i32, i1 } %tmp.62, 0
  %tmp.63 = extractvalue { i32, i1 } %tmp.62, 1
  br i1 %tmp.63, label %zil.trap, label %checked.23
checked.23:
  %tmp.64 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.89, i32 %v.92)
  %v.93 = extractvalue { i32, i1 } %tmp.64, 0
  %tmp.65 = extractvalue { i32, i1 } %tmp.64, 1
  br i1 %tmp.65, label %zil.trap, label %checked.24
checked.24:
  %v.94 = add i32 0, 7
  %tmp.66 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.93, i32 %v.94)
  %v.95 = extractvalue { i32, i1 } %tmp.66, 0
  %tmp.67 = extractvalue { i32, i1 } %tmp.66, 1
  br i1 %tmp.67, label %zil.trap, label %checked.25
checked.25:
  %tmp.68 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.88, i32 %v.95)
  %v.96 = extractvalue { i32, i1 } %tmp.68, 0
  %tmp.69 = extractvalue { i32, i1 } %tmp.68, 1
  br i1 %tmp.69, label %zil.trap, label %checked.26
checked.26:
  %tmp.70 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.70, label %package.drop.27, label %package.done.27
package.drop.27:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.71 = load i1, ptr %package.live.3.0, align 1
  br i1 %tmp.71, label %package.field.drop.28, label %package.field.done.28
package.field.drop.28:
  store i1 false, ptr %package.live.3.0, align 1
  %tmp.72 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.72)
  br label %package.field.done.28
package.field.done.28:
  %tmp.73 = load i1, ptr %package.live.3.1, align 1
  br i1 %tmp.73, label %package.field.drop.29, label %package.field.done.29
package.field.drop.29:
  store i1 false, ptr %package.live.3.1, align 1
  %tmp.74 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.74)
  br label %package.field.done.29
package.field.done.29:
  %tmp.75 = load i1, ptr %package.live.3.3, align 1
  br i1 %tmp.75, label %package.field.drop.30, label %package.field.done.30
package.field.drop.30:
  store i1 false, ptr %package.live.3.3, align 1
  %tmp.76 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } }, ptr %local.3, i32 0, i32 3
  call void @zil.bytes.drop(ptr %tmp.76)
  br label %package.field.done.30
package.field.done.30:
  br label %package.done.27
package.done.27:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %package.live.3.0, align 1
  store i1 false, ptr %package.live.3.1, align 1
  store i1 false, ptr %package.live.3.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  ret i32 %v.96
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.5(ptr %arg.0) {
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

define { { i32 }, i32 } @zil.fn.6(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i32 }, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.4, align 1
  %v.0 = load i32, ptr %local.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.2, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = load i32, ptr %local.0, align 4
  store i32 %v.3, ptr %local.3, align 4
  %v.4 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.4, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.5 = load i32, ptr %local.3, align 4
  %v.6 = load { i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
%tmp.0 = insertvalue { { i32 }, i32 } zeroinitializer, { i32 } %v.6, 0
%v.7 = insertvalue { { i32 }, i32 } %tmp.0, i32 %v.5, 1
  %tmp.1 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.fn.5(ptr %local.4)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.2 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.5(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  ret { { i32 }, i32 } %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.7() {
entry:
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.8(i32 %arg.0) {
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
  call void @zil.fn.5(ptr %local.2)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.2, align 1
  ret %zil_env_0 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.9() {
entry:
  br label %bb.0
bb.0:
  %v.0 = call { ptr, i64, i64 } @zil.fn.7()
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i1 @zil.fn.10(ptr %arg.0, i64 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i64, ptr %local.1, align 8
  %v.2 = call i1 @zil.store.2.reserve(ptr %v.0, i64 %v.1)
  ret i1 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.11(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i1, { { i32 }, i32 }, {} }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = call { { i32 }, i32 } @zil.fn.6(i32 %v.1)
  %v.3 = call { i1, { { i32 }, i32 }, {} } @zil.store.2.push(ptr %v.0, { { i32 }, i32 } %v.2)
  store { i1, { { i32 }, i32 }, {} } %v.3, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %tmp.0 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.0, label %error.drop.0, label %error.done.0
error.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.1 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.2, i32 0, i32 0
  %tmp.2 = load i1, ptr %tmp.1, align 1
  br i1 %tmp.2, label %error.payload.0, label %success.payload.0
error.payload.0:
  %tmp.3 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.2, i32 0, i32 1
  %tmp.4 = getelementptr { { i32 }, i32 }, ptr %tmp.3, i32 0, i32 0
  call void @zil.fn.5(ptr %tmp.4)
  br label %error.done.0
success.payload.0:
  br label %error.done.0
error.done.0:
  store i1 false, ptr %resource.live.2, align 1
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.12(ptr %env) {
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

define { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } } @zil.fn.13(%zil_env_0 %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { ptr, i64, i64 }, { ptr, i64, i64 } }, align 8
  %package.live.1.0 = alloca i1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  %package.live.1.1 = alloca i1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { ptr, i64, i64 }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca %zil_env_0, align 4
  %local.4 = alloca i32, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { ptr, i64, i64 }, align 8
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { ptr, i64, i64 }, align 8
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca %zil_env_0, align 4
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { ptr, i64, i64 }, align 8
  store %zil_env_0 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = call { { ptr, i64, i64 }, { ptr, i64, i64 } } @zil.fn.2()
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store { { ptr, i64, i64 }, { ptr, i64, i64 } } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.0, align 1
  store i1 true, ptr %package.live.1.1, align 1
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  %tmp.0 = getelementptr { { ptr, i64, i64 }, { ptr, i64, i64 } }, ptr %local.1, i32 0, i32 0
  %v.1 = load { ptr, i64, i64 }, ptr %tmp.0, align 8
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store { ptr, i64, i64 } %v.1, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.2 = load %zil_env_0, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.3, align 1
  store %zil_env_0 %v.2, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.3 = add i32 0, 0
  store i32 %v.3, ptr %local.4, align 4
  %tmp.1 = getelementptr { { ptr, i64, i64 }, { ptr, i64, i64 } }, ptr %local.1, i32 0, i32 1
  %v.4 = load { ptr, i64, i64 }, ptr %tmp.1, align 8
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %resource.live.5, align 1
  store { ptr, i64, i64 } %v.4, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.5 = load { ptr, i64, i64 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store { ptr, i64, i64 } %v.5, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  %v.6 = load %zil_env_0, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store %zil_env_0 %v.6, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %v.7 = load i32, ptr %local.4, align 4
  %v.8 = load { ptr, i64, i64 }, ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
  store { ptr, i64, i64 } %v.8, ptr %local.8, align 8
  store i1 true, ptr %resource.live.8, align 1
  %v.9 = load { ptr, i64, i64 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  %v.10 = load %zil_env_0, ptr %local.7, align 4
  store i1 false, ptr %resource.live.7, align 1
  %v.11 = load { ptr, i64, i64 }, ptr %local.8, align 8
  store i1 false, ptr %resource.live.8, align 1
%tmp.2 = insertvalue { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } } zeroinitializer, { ptr, i64, i64 } %v.9, 0
%tmp.3 = insertvalue { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } } %tmp.2, %zil_env_0 %v.10, 1
%tmp.4 = insertvalue { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } } %tmp.3, i32 %v.7, 2
%v.12 = insertvalue { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } } %tmp.4, { ptr, i64, i64 } %v.11, 3
  %tmp.5 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.5, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.bytes.drop(ptr %local.8)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.6 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.6, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.closure.drop.0(ptr %local.7)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.7 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.7, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.store.2.drop(ptr %local.6)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.8 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.8, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.bytes.drop(ptr %local.5)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.9 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.9, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.closure.drop.0(ptr %local.3)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.10 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.10, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.store.2.drop(ptr %local.2)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.11 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.11, label %package.drop.6, label %package.done.6
package.drop.6:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.12 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.12, label %package.field.drop.7, label %package.field.done.7
package.field.drop.7:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.13 = getelementptr { { ptr, i64, i64 }, { ptr, i64, i64 } }, ptr %local.1, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.13)
  br label %package.field.done.7
package.field.done.7:
  %tmp.14 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.14, label %package.field.drop.8, label %package.field.done.8
package.field.drop.8:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.15 = getelementptr { { ptr, i64, i64 }, { ptr, i64, i64 } }, ptr %local.1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.15)
  br label %package.field.done.8
package.field.done.8:
  br label %package.done.6
package.done.6:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.16 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.16, label %closure.drop.9, label %closure.done.9
closure.drop.9:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.9
closure.done.9:
  store i1 false, ptr %resource.live.0, align 1
  ret { { ptr, i64, i64 }, %zil_env_0, i32, { ptr, i64, i64 } } %v.12
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.3()
  ret i32 %entry.result
}
