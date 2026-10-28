; ZIL verified LLVM module
; verified-fingerprint: 20b7af205142349ba90b24e781f941fe704bda8a4c1a424f661044a865653d4d
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
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

define internal i1 @zil.store.1.reserve(ptr %head, i64 %requested) {
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
define internal { i1, { { i32 }, i32 }, {} } @zil.store.1.push(ptr %head, { { i32 }, i32 } %item) {
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
define internal { i1, {}, { { i32 }, i32 } } @zil.store.1.pop(ptr %head) {
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
define internal void @zil.store.1.drop(ptr %head) {
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
  call void @zil.fn.1(ptr %field.0)
  %next = add i64 %index, 1
  br label %loop
release:
  call void @zil.bytes.release(ptr %data)
  ret void
}

define internal i1 @zil.store.2.reserve(ptr %head, i64 %requested) {
entry:
  %stride = ptrtoint ptr getelementptr ({ { i32 }, i64 }, ptr null, i64 1) to i64
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
define internal { i1, { { i32 }, i64 }, {} } @zil.store.2.push(ptr %head, { { i32 }, i64 } %item) {
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
  %slot = getelementptr { { i32 }, i64 }, ptr %data, i64 %len
  store { { i32 }, i64 } %item, ptr %slot, align 8
  %next.len = add i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  ret { i1, { { i32 }, i64 }, {} } zeroinitializer
full:
  %tag = insertvalue { i1, { { i32 }, i64 }, {} } zeroinitializer, i1 true, 0
  %error = insertvalue { i1, { { i32 }, i64 }, {} } %tag, { { i32 }, i64 } %item, 1
  ret { i1, { { i32 }, i64 }, {} } %error
}
define internal { i1, {}, { { i32 }, i64 } } @zil.store.2.pop(ptr %head) {
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
  %slot = getelementptr { { i32 }, i64 }, ptr %data, i64 %next.len
  %item = load { { i32 }, i64 }, ptr %slot, align 8
  %result = insertvalue { i1, {}, { { i32 }, i64 } } zeroinitializer, { { i32 }, i64 } %item, 2
  ret { i1, {}, { { i32 }, i64 } } %result
empty:
  %error = insertvalue { i1, {}, { { i32 }, i64 } } zeroinitializer, i1 true, 0
  ret { i1, {}, { { i32 }, i64 } } %error
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
  %slot = getelementptr { { i32 }, i64 }, ptr %data, i64 %index
  %field.0 = getelementptr { { i32 }, i64 }, ptr %slot, i32 0, i32 0
  call void @zil.fn.1(ptr %field.0)
  %next = add i64 %index, 1
  br label %loop
release:
  call void @zil.bytes.release(ptr %data)
  ret void
}
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

define { { i32 }, i32 } @zil.fn.2(i32 %arg.0) {
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
  call void @zil.fn.1(ptr %local.4)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.2 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  ret { { i32 }, i32 } %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, i64 } @zil.fn.3(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca i64, align 8
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
  %v.4 = sext i32 %v.3 to i64
  store i64 %v.4, ptr %local.3, align 8
  %v.5 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.5, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.6 = load i64, ptr %local.3, align 8
  %v.7 = load { i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
%tmp.0 = insertvalue { { i32 }, i64 } zeroinitializer, { i32 } %v.7, 0
%v.8 = insertvalue { { i32 }, i64 } %tmp.0, i64 %v.6, 1
  %tmp.1 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.fn.1(ptr %local.4)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.2 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  ret { { i32 }, i64 } %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.4() {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = insertvalue %zil_env_0 zeroinitializer, i32 %v.1, 0
  ret %zil_env_0 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, i64 }, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { ptr, i64, i64 } }, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { ptr, i64, i64 }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { ptr, i64, i64 }, %zil_env_0, i32 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { { ptr, i64, i64 }, %zil_env_0, i32 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %v.0 = add i32 0, 67
  %v.1 = call { { i32 }, i64 } @zil.fn.3(i32 %v.0)
  store { { i32 }, i64 } %v.1, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.2 = load { { i32 }, i64 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  %v.3 = call { { ptr, i64, i64 } } @zil.fn.9({ { i32 }, i64 } %v.2)
  store { { ptr, i64, i64 } } %v.3, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.4 = load { { ptr, i64, i64 } }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  %v.5 = call { ptr, i64, i64 } @zil.fn.10({ { ptr, i64, i64 } } %v.4)
  %tmp.0 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.1 = getelementptr { { ptr, i64, i64 } }, ptr %local.1, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.1)
  br label %package.done.0
package.done.0:
  %tmp.2 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.2, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.3 = getelementptr { { i32 }, i64 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.3)
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.2, align 1
  store { ptr, i64, i64 } %v.5, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.6 = call %zil_env_0 @zil.fn.4()
  %v.7 = call { { ptr, i64, i64 }, %zil_env_0, i32 } @zil.fn.16(%zil_env_0 %v.6)
  store { { ptr, i64, i64 }, %zil_env_0, i32 } %v.7, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.8 = load { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  %v.9 = call { { ptr, i64, i64 }, %zil_env_0, i32 } @zil.fn.17({ { ptr, i64, i64 }, %zil_env_0, i32 } %v.8)
  %tmp.4 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.4, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.5 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.store.1.drop(ptr %tmp.5)
  %tmp.6 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.6)
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.4, align 1
  store { { ptr, i64, i64 }, %zil_env_0, i32 } %v.9, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %tmp.7 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.4, i32 0, i32 2
  %v.10 = load i32, ptr %tmp.7, align 4
  %v.11 = getelementptr i8, ptr %local.2, i64 0
  %v.12 = call i64 @zil.fn.11(ptr %v.11)
  %tmp.8 = icmp slt i64 %v.12, -2147483648
  %tmp.9 = icmp sgt i64 %v.12, 2147483647
  %tmp.10 = or i1 %tmp.8, %tmp.9
  br i1 %tmp.10, label %zil.trap, label %checked.3
checked.3:
  %v.13 = trunc i64 %v.12 to i32
  %tmp.11 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.10, i32 %v.13)
  %v.14 = extractvalue { i32, i1 } %tmp.11, 0
  %tmp.12 = extractvalue { i32, i1 } %tmp.11, 1
  br i1 %tmp.12, label %zil.trap, label %checked.4
checked.4:
  %v.15 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.4, i32 0, i32 0
  %tmp.13 = getelementptr { ptr, i64, i64 }, ptr %v.15, i32 0, i32 1
  %v.16 = load i64, ptr %tmp.13, align 8
  %tmp.14 = icmp slt i64 %v.16, -2147483648
  %tmp.15 = icmp sgt i64 %v.16, 2147483647
  %tmp.16 = or i1 %tmp.14, %tmp.15
  br i1 %tmp.16, label %zil.trap, label %checked.5
checked.5:
  %v.17 = trunc i64 %v.16 to i32
  %tmp.17 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.14, i32 %v.17)
  %v.18 = extractvalue { i32, i1 } %tmp.17, 0
  %tmp.18 = extractvalue { i32, i1 } %tmp.17, 1
  br i1 %tmp.18, label %zil.trap, label %checked.6
checked.6:
  %v.19 = add i32 0, 3
  %tmp.19 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.18, i32 %v.19)
  %v.20 = extractvalue { i32, i1 } %tmp.19, 0
  %tmp.20 = extractvalue { i32, i1 } %tmp.19, 1
  br i1 %tmp.20, label %zil.trap, label %checked.7
checked.7:
  %tmp.21 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.21, label %package.drop.8, label %package.done.8
package.drop.8:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.22 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.4, i32 0, i32 0
  call void @zil.store.1.drop(ptr %tmp.22)
  %tmp.23 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.4, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.23)
  br label %package.done.8
package.done.8:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  %tmp.24 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.24, label %resource.drop.9, label %resource.done.9
resource.drop.9:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.store.2.drop(ptr %local.2)
  br label %resource.done.9
resource.done.9:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.20
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.6() {
entry:
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i1 @zil.fn.7(ptr %arg.0, i64 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i64, ptr %local.1, align 8
  %v.2 = call i1 @zil.store.1.reserve(ptr %v.0, i64 %v.1)
  ret i1 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { { i32 }, i32 }, {} } @zil.fn.8(ptr %arg.0, { { i32 }, i32 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 true, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, i32 }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store { { i32 }, i32 } %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load { { i32 }, i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %v.2 = call { i1, { { i32 }, i32 }, {} } @zil.store.1.push(ptr %v.0, { { i32 }, i32 } %v.1)
  %tmp.0 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.1 = getelementptr { { i32 }, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.1)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, { { i32 }, i32 }, {} } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { ptr, i64, i64 } } @zil.fn.9({ { i32 }, i64 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, i64 }, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { ptr, i64, i64 }, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { i32 }, i64 }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i1, { { i32 }, i64 }, {} }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { ptr, i64, i64 }, align 8
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { ptr, i64, i64 }, align 8
  store { { i32 }, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = call { ptr, i64, i64 } @zil.fn.12()
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = getelementptr i8, ptr %local.1, i64 0
  %v.2 = add i64 0, 1
  %v.3 = call i1 @zil.fn.13(ptr %v.1, i64 %v.2)
  %v.4 = getelementptr i8, ptr %local.1, i64 0
  %v.5 = load { { i32 }, i64 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store { { i32 }, i64 } %v.5, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.6 = load { { i32 }, i64 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  %v.7 = call { i1, { { i32 }, i64 }, {} } @zil.fn.14(ptr %v.4, { { i32 }, i64 } %v.6)
  store { i1, { { i32 }, i64 }, {} } %v.7, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %tmp.0 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.0, label %error.drop.0, label %error.done.0
error.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.1 = getelementptr { i1, { { i32 }, i64 }, {} }, ptr %local.3, i32 0, i32 0
  %tmp.2 = load i1, ptr %tmp.1, align 1
  br i1 %tmp.2, label %error.payload.0, label %success.payload.0
error.payload.0:
  %tmp.3 = getelementptr { i1, { { i32 }, i64 }, {} }, ptr %local.3, i32 0, i32 1
  %tmp.4 = getelementptr { { i32 }, i64 }, ptr %tmp.3, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.4)
  br label %error.done.0
success.payload.0:
  br label %error.done.0
error.done.0:
  %tmp.5 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.5, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.6 = getelementptr { { i32 }, i64 }, ptr %local.2, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.6)
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.5, align 1
  %v.8 = load { ptr, i64, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.8, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.9 = load { ptr, i64, i64 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.9, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.10 = load { ptr, i64, i64 }, ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
%v.11 = insertvalue { { ptr, i64, i64 } } zeroinitializer, { ptr, i64, i64 } %v.10, 0
  %tmp.7 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.7, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.store.2.drop(ptr %local.5)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.8 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.8, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.store.2.drop(ptr %local.4)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.9 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.9, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.store.2.drop(ptr %local.1)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.10 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.10, label %package.drop.5, label %package.done.5
package.drop.5:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.11 = getelementptr { { i32 }, i64 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.11)
  br label %package.done.5
package.done.5:
  store i1 false, ptr %resource.live.0, align 1
  ret { { ptr, i64, i64 } } %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.10({ { ptr, i64, i64 } } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { ptr, i64, i64 } }, align 8
  %package.live.0.0 = alloca i1, align 1
  store i1 true, ptr %package.live.0.0, align 1
  store { { ptr, i64, i64 } } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr { { ptr, i64, i64 } }, ptr %local.0, i32 0, i32 0
  %v.0 = load { ptr, i64, i64 }, ptr %tmp.0, align 8
  store i1 false, ptr %package.live.0.0, align 1
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.2 = load i1, ptr %package.live.0.0, align 1
  br i1 %tmp.2, label %package.field.drop.1, label %package.field.done.1
package.field.drop.1:
  store i1 false, ptr %package.live.0.0, align 1
  %tmp.3 = getelementptr { { ptr, i64, i64 } }, ptr %local.0, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.3)
  br label %package.field.done.1
package.field.done.1:
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %package.live.0.0, align 1
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.11(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.0, i32 0, i32 1
  %v.1 = load i64, ptr %tmp.0, align 8
  ret i64 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.12() {
entry:
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i1 @zil.fn.13(ptr %arg.0, i64 %arg.1) {
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

define { i1, { { i32 }, i64 }, {} } @zil.fn.14(ptr %arg.0, { { i32 }, i64 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 true, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, i64 }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store { { i32 }, i64 } %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load { { i32 }, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  %v.2 = call { i1, { { i32 }, i64 }, {} } @zil.store.2.push(ptr %v.0, { { i32 }, i64 } %v.1)
  %tmp.0 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.1 = getelementptr { { i32 }, i64 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.1)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, { { i32 }, i64 }, {} } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.15(ptr %env) {
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

define { { ptr, i64, i64 }, %zil_env_0, i32 } @zil.fn.16(%zil_env_0 %arg.0) {
entry:
  %local.0 = alloca %zil_env_0, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { ptr, i64, i64 }, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { i32 }, i32 }, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i1, { { i32 }, i32 }, {} }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { ptr, i64, i64 }, align 8
  %local.5 = alloca %zil_env_0, align 4
  %local.6 = alloca i32, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { ptr, i64, i64 }, align 8
  store %zil_env_0 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = call { ptr, i64, i64 } @zil.fn.6()
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = getelementptr i8, ptr %local.1, i64 0
  %v.2 = add i64 0, 1
  %v.3 = call i1 @zil.fn.7(ptr %v.1, i64 %v.2)
  %v.4 = getelementptr i8, ptr %local.1, i64 0
  %v.5 = add i32 0, 65
  %v.6 = call { { i32 }, i32 } @zil.fn.2(i32 %v.5)
  store { { i32 }, i32 } %v.6, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.7 = load { { i32 }, i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.8 = call { i1, { { i32 }, i32 }, {} } @zil.fn.8(ptr %v.4, { { i32 }, i32 } %v.7)
  store { i1, { { i32 }, i32 }, {} } %v.8, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %tmp.0 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.0, label %error.drop.0, label %error.done.0
error.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.1 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.3, i32 0, i32 0
  %tmp.2 = load i1, ptr %tmp.1, align 1
  br i1 %tmp.2, label %error.payload.0, label %success.payload.0
error.payload.0:
  %tmp.3 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.3, i32 0, i32 1
  %tmp.4 = getelementptr { { i32 }, i32 }, ptr %tmp.3, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.4)
  br label %error.done.0
success.payload.0:
  br label %error.done.0
error.done.0:
  %tmp.5 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.5, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.6 = getelementptr { { i32 }, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.6)
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.7, align 1
  %v.9 = load { ptr, i64, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.9, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.10 = load %zil_env_0, ptr %local.0, align 4
  store %zil_env_0 %v.10, ptr %local.5, align 4
  %v.11 = add i32 0, 0
  store i32 %v.11, ptr %local.6, align 4
  %v.12 = load { ptr, i64, i64 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.12, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  %v.13 = load %zil_env_0, ptr %local.5, align 4
  %v.14 = load i32, ptr %local.6, align 4
  %v.15 = load { ptr, i64, i64 }, ptr %local.7, align 8
  store i1 false, ptr %resource.live.7, align 1
%tmp.7 = insertvalue { { ptr, i64, i64 }, %zil_env_0, i32 } zeroinitializer, { ptr, i64, i64 } %v.15, 0
%tmp.8 = insertvalue { { ptr, i64, i64 }, %zil_env_0, i32 } %tmp.7, %zil_env_0 %v.13, 1
%v.16 = insertvalue { { ptr, i64, i64 }, %zil_env_0, i32 } %tmp.8, i32 %v.14, 2
  %tmp.9 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.9, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.store.1.drop(ptr %local.7)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.10 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.10, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.store.1.drop(ptr %local.4)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.11 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.11, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.store.1.drop(ptr %local.1)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.1, align 1
  ret { { ptr, i64, i64 }, %zil_env_0, i32 } %v.16
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { ptr, i64, i64 }, %zil_env_0, i32 } @zil.fn.17({ { ptr, i64, i64 }, %zil_env_0, i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { ptr, i64, i64 }, %zil_env_0, i32 }, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { ptr, i64, i64 }, %zil_env_0, i32 }, align 8
  %package.live.1.0 = alloca i1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  %package.live.1.1 = alloca i1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { ptr, i64, i64 }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { ptr, i64, i64 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { { i32 }, i32 }, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i1, { { i32 }, i32 }, {} }, align 4
  %local.6 = alloca ptr, align 8
  store { { ptr, i64, i64 }, %zil_env_0, i32 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = load { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store { { ptr, i64, i64 }, %zil_env_0, i32 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.0, align 1
  store i1 true, ptr %package.live.1.1, align 1
  %tmp.0 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 0
  %v.1 = load { ptr, i64, i64 }, ptr %tmp.0, align 8
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store { ptr, i64, i64 } %v.1, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.2 = call { ptr, i64, i64 } @zil.fn.6()
  store { ptr, i64, i64 } %v.2, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %tmp.1 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.1, label %package.field.drop.0, label %package.field.done.0
package.field.drop.0:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.2 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.store.1.drop(ptr %tmp.2)
  br label %package.field.done.0
package.field.done.0:
  %v.3 = load { ptr, i64, i64 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  %tmp.3 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 0
  store { ptr, i64, i64 } %v.3, ptr %tmp.3, align 8
  store i1 true, ptr %package.live.1.0, align 1
  %tmp.4 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.4, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.store.1.drop(ptr %local.3)
  br label %resource.done.1
resource.done.1:
  %v.4 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 0
  %v.5 = add i64 0, 1
  %v.6 = call i1 @zil.fn.7(ptr %v.4, i64 %v.5)
  %v.7 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 0
  %v.8 = add i32 0, 66
  %v.9 = call { { i32 }, i32 } @zil.fn.2(i32 %v.8)
  store { { i32 }, i32 } %v.9, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.10 = load { { i32 }, i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  %v.11 = call { i1, { { i32 }, i32 }, {} } @zil.fn.8(ptr %v.7, { { i32 }, i32 } %v.10)
  store { i1, { { i32 }, i32 }, {} } %v.11, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %tmp.5 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.5, label %error.drop.2, label %error.done.2
error.drop.2:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.6 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.5, i32 0, i32 0
  %tmp.7 = load i1, ptr %tmp.6, align 1
  br i1 %tmp.7, label %error.payload.2, label %success.payload.2
error.payload.2:
  %tmp.8 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.5, i32 0, i32 1
  %tmp.9 = getelementptr { { i32 }, i32 }, ptr %tmp.8, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.9)
  br label %error.done.2
success.payload.2:
  br label %error.done.2
error.done.2:
  %tmp.10 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.10, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.11 = getelementptr { { i32 }, i32 }, ptr %local.4, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.11)
  br label %package.done.3
package.done.3:
  %v.12 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 1
  %v.13 = getelementptr i8, ptr %v.12, i64 0
  store ptr %v.13, ptr %local.6, align 8
  %v.14 = load ptr, ptr %local.6, align 8
  %v.15 = call i32 @zil.fn.15(ptr %v.14)
  %tmp.12 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 2
  store i32 %v.15, ptr %tmp.12, align 4
  %v.16 = load { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  %tmp.13 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.13, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.store.1.drop(ptr %local.2)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.14 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.14, label %package.drop.5, label %package.done.5
package.drop.5:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.15 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.15, label %package.field.drop.6, label %package.field.done.6
package.field.drop.6:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.16 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.store.1.drop(ptr %tmp.16)
  br label %package.field.done.6
package.field.done.6:
  %tmp.17 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.17, label %package.field.drop.7, label %package.field.done.7
package.field.drop.7:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.18 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.18)
  br label %package.field.done.7
package.field.done.7:
  br label %package.done.5
package.done.5:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.19 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.19, label %package.drop.8, label %package.done.8
package.drop.8:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.20 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.store.1.drop(ptr %tmp.20)
  %tmp.21 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.21)
  br label %package.done.8
package.done.8:
  store i1 false, ptr %resource.live.0, align 1
  ret { { ptr, i64, i64 }, %zil_env_0, i32 } %v.16
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.5()
  ret i32 %entry.result
}
