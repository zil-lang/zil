; ZIL verified LLVM module
; verified-fingerprint: c085a314cd8dd3045b5f16735b124eed29f17329c2aac05c141cc48acdabceb7
target triple = "x86_64-unknown-linux-gnu"

@zil.wstr.5.20 = private unnamed_addr constant [4 x i16] [i16 90, i16 73, i16 76, i16 0], align 2
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
  %stride = ptrtoint ptr getelementptr ({ { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, ptr null, i64 1) to i64
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
define internal { i1, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, {} } @zil.store.1.push(ptr %head, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } %item) {
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
  %slot = getelementptr { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, ptr %data, i64 %len
  store { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } %item, ptr %slot, align 8
  %next.len = add i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  ret { i1, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, {} } zeroinitializer
full:
  %tag = insertvalue { i1, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, {} } zeroinitializer, i1 true, 0
  %error = insertvalue { i1, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, {} } %tag, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } %item, 1
  ret { i1, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, {} } %error
}
define internal { i1, {}, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } } @zil.store.1.pop(ptr %head) {
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
  %slot = getelementptr { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, ptr %data, i64 %next.len
  %item = load { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, ptr %slot, align 8
  %result = insertvalue { i1, {}, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } } zeroinitializer, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } %item, 2
  ret { i1, {}, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } } %result
empty:
  %error = insertvalue { i1, {}, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } } zeroinitializer, i1 true, 0
  ret { i1, {}, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } } %error
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
  %slot = getelementptr { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, ptr %data, i64 %index
  %field.0 = getelementptr { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, ptr %slot, i32 0, i32 0
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

define i32 @zil.fn.4(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  ret i32 %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { ptr, i64, i64 }, align 8
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca { i32, i1 }, align 4
  %local.4 = alloca [2 x i8], align 1
  %local.5 = alloca { { i32, i1 }, [2 x i8] }, align 4
  %local.6 = alloca { i32, [1 x i32] }, align 4
  %local.7 = alloca ptr, align 8
  %local.8 = alloca ptr, align 8
  %local.9 = alloca ptr, align 8
  %resource.live.10 = alloca i1, align 1
  store i1 false, ptr %resource.live.10, align 1
  %local.10 = alloca { i32 }, align 4
  %resource.live.11 = alloca i1, align 1
  store i1 false, ptr %resource.live.11, align 1
  %local.11 = alloca { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, align 8
  %resource.live.12 = alloca i1, align 1
  store i1 false, ptr %resource.live.12, align 1
  %local.12 = alloca { i1, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, {} }, align 8
  %enum.slot.18 = alloca { i32, [1 x i32] }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.12, align 1
  store i1 false, ptr %resource.live.11, align 1
  %v.0 = call { ptr, i64, i64 } @zil.fn.6()
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = add i64 0, 1
  %v.3 = call i1 @zil.fn.7(ptr %v.1, i64 %v.2)
  %v.4 = getelementptr i8, ptr %local.0, i64 0
  store i1 false, ptr %resource.live.10, align 1
  %v.5 = add i32 0, 65
  store i32 %v.5, ptr %local.1, align 4
  %v.6 = load i32, ptr %local.1, align 4
%v.7 = insertvalue { i32 } zeroinitializer, i32 %v.6, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.7, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.8 = add i32 0, 7
  %v.9 = or i1 false, true
%tmp.0 = insertvalue { i32, i1 } zeroinitializer, i32 %v.8, 0
%v.10 = insertvalue { i32, i1 } %tmp.0, i1 %v.9, 1
  store { i32, i1 } %v.10, ptr %local.3, align 4
  %v.11 = add i8 0, 1
  %v.12 = add i8 0, 2
%tmp.1 = insertvalue [2 x i8] zeroinitializer, i8 %v.11, 0
%v.13 = insertvalue [2 x i8] %tmp.1, i8 %v.12, 1
  store [2 x i8] %v.13, ptr %local.4, align 1
  %v.14 = load { i32, i1 }, ptr %local.3, align 4
  %v.15 = load [2 x i8], ptr %local.4, align 1
%tmp.2 = insertvalue { { i32, i1 }, [2 x i8] } zeroinitializer, { i32, i1 } %v.14, 0
%v.16 = insertvalue { { i32, i1 }, [2 x i8] } %tmp.2, [2 x i8] %v.15, 1
  store { { i32, i1 }, [2 x i8] } %v.16, ptr %local.5, align 4
  %v.17 = add i32 0, 3
  store { i32, [1 x i32] } zeroinitializer, ptr %enum.slot.18, align 4
  %tmp.3 = getelementptr { i32, [1 x i32] }, ptr %enum.slot.18, i32 0, i32 0
  store i32 1, ptr %tmp.3, align 4
  %tmp.4 = getelementptr { i32, [1 x i32] }, ptr %enum.slot.18, i32 0, i32 1
  %tmp.5 = getelementptr { i32 }, ptr %tmp.4, i32 0, i32 0
  store i32 %v.17, ptr %tmp.5, align 4
  %v.18 = load { i32, [1 x i32] }, ptr %enum.slot.18, align 4
  store { i32, [1 x i32] } %v.18, ptr %local.6, align 4
  %v.19 = freeze ptr null
  store ptr %v.19, ptr %local.7, align 8
  %v.20 = freeze ptr @zil.wstr.5.20
  store ptr %v.20, ptr %local.8, align 8
  %v.21 = freeze ptr @zil.fn.4
  store ptr %v.21, ptr %local.9, align 8
  %v.22 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.22, ptr %local.10, align 4
  store i1 true, ptr %resource.live.10, align 1
  %v.23 = load { { i32, i1 }, [2 x i8] }, ptr %local.5, align 4
  %v.24 = load { i32, [1 x i32] }, ptr %local.6, align 4
  %v.25 = load ptr, ptr %local.7, align 8
  %v.26 = load ptr, ptr %local.8, align 8
  %v.27 = load ptr, ptr %local.9, align 8
  %v.28 = load { i32 }, ptr %local.10, align 4
  store i1 false, ptr %resource.live.10, align 1
%tmp.6 = insertvalue { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } zeroinitializer, { i32 } %v.28, 0
%tmp.7 = insertvalue { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } %tmp.6, { { i32, i1 }, [2 x i8] } %v.23, 1
%tmp.8 = insertvalue { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } %tmp.7, { i32, [1 x i32] } %v.24, 2
%tmp.9 = insertvalue { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } %tmp.8, ptr %v.25, 3
%tmp.10 = insertvalue { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } %tmp.9, ptr %v.26, 4
%v.29 = insertvalue { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } %tmp.10, ptr %v.27, 5
  %tmp.11 = load i1, ptr %resource.live.10, align 1
  br i1 %tmp.11, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.10, align 1
  call void @zil.fn.1(ptr %local.10)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.10, align 1
  %tmp.12 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.12, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  store { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } %v.29, ptr %local.11, align 8
  store i1 true, ptr %resource.live.11, align 1
  %v.30 = load { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, ptr %local.11, align 8
  store i1 false, ptr %resource.live.11, align 1
  %v.31 = call { i1, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, {} } @zil.fn.8(ptr %v.4, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } %v.30)
  store { i1, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, {} } %v.31, ptr %local.12, align 8
  store i1 true, ptr %resource.live.12, align 1
  %tmp.13 = load i1, ptr %resource.live.12, align 1
  br i1 %tmp.13, label %error.drop.2, label %error.done.2
error.drop.2:
  store i1 false, ptr %resource.live.12, align 1
  %tmp.14 = getelementptr { i1, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, {} }, ptr %local.12, i32 0, i32 0
  %tmp.15 = load i1, ptr %tmp.14, align 1
  br i1 %tmp.15, label %error.payload.2, label %success.payload.2
error.payload.2:
  %tmp.16 = getelementptr { i1, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, {} }, ptr %local.12, i32 0, i32 1
  %tmp.17 = getelementptr { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, ptr %tmp.16, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.17)
  br label %error.done.2
success.payload.2:
  br label %error.done.2
error.done.2:
  %tmp.18 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.18, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.11, align 1
  %tmp.19 = getelementptr { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, ptr %local.11, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.19)
  br label %package.done.3
package.done.3:
  %v.32 = getelementptr i8, ptr %local.0, i64 0
  %v.33 = call i64 @zil.fn.9(ptr %v.32)
  %tmp.20 = icmp slt i64 %v.33, -2147483648
  %tmp.21 = icmp sgt i64 %v.33, 2147483647
  %tmp.22 = or i1 %tmp.20, %tmp.21
  br i1 %tmp.22, label %zil.trap, label %checked.4
checked.4:
  %v.34 = trunc i64 %v.33 to i32
  %v.35 = add i32 0, 1
  %tmp.23 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.34, i32 %v.35)
  %v.36 = extractvalue { i32, i1 } %tmp.23, 0
  %tmp.24 = extractvalue { i32, i1 } %tmp.23, 1
  br i1 %tmp.24, label %zil.trap, label %checked.5
checked.5:
  store i1 false, ptr %resource.live.12, align 1
  store i1 false, ptr %resource.live.11, align 1
  %tmp.25 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.25, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.36
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

define { i1, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, {} } @zil.fn.8(ptr %arg.0, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 true, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  %v.2 = call { i1, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, {} } @zil.store.1.push(ptr %v.0, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr } %v.1)
  %tmp.0 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.1 = getelementptr { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.1)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, { { i32 }, { { i32, i1 }, [2 x i8] }, { i32, [1 x i32] }, ptr, ptr, ptr }, {} } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.9(ptr %arg.0) {
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

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.5()
  ret i32 %entry.result
}
