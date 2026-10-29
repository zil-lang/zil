; ZIL verified LLVM module
; verified-fingerprint: efcf934f778015e5b709b9d556f027568152121e99402e08d81a35608f6f405f
target triple = "x86_64-unknown-linux-gnu"

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

define { ptr, i64, i64 } @zil.fn.3() {
entry:
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, {}, i32 } @zil.fn.4() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { ptr, i64, i64 }, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i1, { { i32 }, i32 }, {} }, align 4
  %local.2 = alloca i1, align 1
  %local.3 = alloca i1, align 1
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { { i32 }, i32 }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.1, align 1
  %v.0 = call { ptr, i64, i64 } @zil.fn.3()
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = add i64 0, 1
  %v.3 = call i1 @zil.store.1.reserve(ptr %v.1, i64 %v.2)
  %v.4 = xor i1 %v.3, true
  br i1 %v.4, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.6 = insertvalue { i1, {}, i32 } %tmp.0, {} zeroinitializer, 1
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, {}, i32 } %v.6
bb.2:
  br label %bb.3
bb.3:
  %v.7 = getelementptr i8, ptr %local.0, i64 0
  %v.8 = add i32 0, 65
  %v.9 = call { { i32 }, i32 } @zil.fn.2(i32 %v.8)
  %v.10 = call { i1, { { i32 }, i32 }, {} } @zil.store.1.push(ptr %v.7, { { i32 }, i32 } %v.9)
  store { i1, { { i32 }, i32 }, {} } %v.10, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %error.drop.1, label %error.done.1
error.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.3 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.1, i32 0, i32 0
  %tmp.4 = load i1, ptr %tmp.3, align 1
  br i1 %tmp.4, label %error.payload.1, label %success.payload.1
error.payload.1:
  %tmp.5 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.1, i32 0, i32 1
  %tmp.6 = getelementptr { { i32 }, i32 }, ptr %tmp.5, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.6)
  br label %error.done.1
success.payload.1:
  br label %error.done.1
error.done.1:
  %v.11 = getelementptr i8, ptr %local.0, i64 0
  %v.12 = add i64 0, -1
  %v.13 = call i1 @zil.store.1.reserve(ptr %v.11, i64 %v.12)
  br i1 %v.13, label %bb.4, label %bb.5
bb.4:
  %v.14 = or i1 false, true
  store i1 %v.14, ptr %local.2, align 1
  br label %bb.6
bb.5:
  %v.15 = getelementptr i8, ptr %local.0, i64 0
  %v.16 = add i64 0, 9223372036854775807
  %v.17 = call i1 @zil.store.1.reserve(ptr %v.15, i64 %v.16)
  store i1 %v.17, ptr %local.2, align 1
  br label %bb.6
bb.6:
  %v.18 = load i1, ptr %local.2, align 1
  br i1 %v.18, label %bb.7, label %bb.8
bb.7:
  %tmp.7 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.20 = insertvalue { i1, {}, i32 } %tmp.7, {} zeroinitializer, 1
  store i1 false, ptr %resource.live.1, align 1
  %tmp.8 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.8, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.20
bb.8:
  br label %bb.9
bb.9:
  %v.21 = getelementptr i8, ptr %local.0, i64 0
  %tmp.9 = getelementptr { ptr, i64, i64 }, ptr %v.21, i32 0, i32 1
  %v.22 = load i64, ptr %tmp.9, align 8
  %v.23 = add i64 0, 1
  %v.24 = icmp ne i64 %v.22, %v.23
  br i1 %v.24, label %bb.10, label %bb.11
bb.10:
  %v.25 = or i1 false, true
  store i1 %v.25, ptr %local.3, align 1
  br label %bb.12
bb.11:
  %v.26 = getelementptr i8, ptr %local.0, i64 0
  %tmp.10 = getelementptr { ptr, i64, i64 }, ptr %v.26, i32 0, i32 2
  %v.27 = load i64, ptr %tmp.10, align 8
  %v.28 = add i64 0, 1
  %v.29 = icmp ne i64 %v.27, %v.28
  store i1 %v.29, ptr %local.3, align 1
  br label %bb.12
bb.12:
  %v.30 = load i1, ptr %local.3, align 1
  br i1 %v.30, label %bb.13, label %bb.14
bb.13:
  %tmp.11 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.32 = insertvalue { i1, {}, i32 } %tmp.11, {} zeroinitializer, 1
  store i1 false, ptr %resource.live.1, align 1
  %tmp.12 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.12, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.32
bb.14:
  br label %bb.15
bb.15:
  %v.33 = getelementptr i8, ptr %local.0, i64 0
  %v.34 = call { i1, {}, { { i32 }, i32 } } @zil.store.1.pop(ptr %v.33)
  %v.35 = extractvalue { i1, {}, { { i32 }, i32 } } %v.34, 0
  br i1 %v.35, label %bb.16, label %bb.17
bb.16:
  %tmp.13 = extractvalue { i1, {}, { { i32 }, i32 } } %v.34, 0
  %tmp.14 = icmp ne i1 %tmp.13, true
  br i1 %tmp.14, label %zil.trap, label %checked.4
checked.4:
  %tmp.15 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.37 = insertvalue { i1, {}, i32 } %tmp.15, {} zeroinitializer, 1
  store i1 false, ptr %resource.live.1, align 1
  %tmp.16 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.16, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.37
bb.17:
  %tmp.17 = extractvalue { i1, {}, { { i32 }, i32 } } %v.34, 0
  %tmp.18 = icmp ne i1 %tmp.17, false
  br i1 %tmp.18, label %zil.trap, label %checked.6
checked.6:
  %v.38 = extractvalue { i1, {}, { { i32 }, i32 } } %v.34, 2
  store i1 false, ptr %resource.live.4, align 1
  store { { i32 }, i32 } %v.38, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %tmp.19 = getelementptr { { i32 }, i32 }, ptr %local.4, i32 0, i32 1
  %v.39 = load i32, ptr %tmp.19, align 4
  %v.40 = add i32 0, 65
  %tmp.20 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.39, i32 %v.40)
  %v.41 = extractvalue { i32, i1 } %tmp.20, 0
  %tmp.21 = extractvalue { i32, i1 } %tmp.20, 1
  br i1 %tmp.21, label %zil.trap, label %checked.7
checked.7:
  %tmp.22 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.42 = insertvalue { i1, {}, i32 } %tmp.22, i32 %v.41, 2
  %tmp.23 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.23, label %package.drop.8, label %package.done.8
package.drop.8:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.24 = getelementptr { { i32 }, i32 }, ptr %local.4, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.24)
  br label %package.done.8
package.done.8:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.1, align 1
  %tmp.25 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.25, label %resource.drop.9, label %resource.done.9
resource.drop.9:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.9
resource.done.9:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.42
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, {}, i32 } @zil.fn.4()
  %entry.error = extractvalue { i1, {}, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, {}, i32 } %entry.result, 2
  ret i32 %entry.success
}
