; ZIL verified LLVM module
; verified-fingerprint: 7a0f8bc0567194a50d20e7937fea786517599db7df0a22e1191cbd7be0a0c5dd
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
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { ptr, i64, i64 }, align 8
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { ptr, i64, i64 }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { ptr, i64, i64 }, i32 }, align 8
  %local.4 = alloca ptr, align 8
  %local.5 = alloca { ptr, i64 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = add i32 0, 40
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = load { ptr, i64, i64 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.2, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = load i32, ptr %local.1, align 4
  %v.4 = load { ptr, i64, i64 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
%tmp.0 = insertvalue { { ptr, i64, i64 }, i32 } zeroinitializer, { ptr, i64, i64 } %v.4, 0
%v.5 = insertvalue { { ptr, i64, i64 }, i32 } %tmp.0, i32 %v.3, 1
  %tmp.1 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.bytes.drop(ptr %local.2)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.2 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.bytes.drop(ptr %local.0)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.3, align 1
  store { { ptr, i64, i64 }, i32 } %v.5, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.6 = getelementptr { { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 1
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  store ptr %v.7, ptr %local.4, align 8
  %v.8 = getelementptr { { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  %v.9 = add i64 0, 1
  %v.10 = call i1 @zil.bytes.reserve(ptr %v.8, i64 %v.9)
  %v.11 = xor i1 %v.10, true
  br i1 %v.11, label %bb.1, label %bb.2
bb.1:
  %v.12 = add i32 0, 8
  %tmp.3 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.3, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.4 = getelementptr { { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.4)
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.3, align 1
  ret i32 %v.12
bb.2:
  br label %bb.3
bb.3:
  %v.13 = getelementptr { { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  %v.14 = add i8 0, 2
  %v.15 = call i1 @zil.bytes.push(ptr %v.13, i8 %v.14)
  %v.16 = getelementptr { { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  %tmp.5 = load ptr, ptr %v.16, align 8
  %tmp.6 = getelementptr { ptr, i64, i64 }, ptr %v.16, i32 0, i32 1
  %tmp.7 = load i64, ptr %tmp.6, align 8
  %tmp.8 = insertvalue { ptr, i64 } poison, ptr %tmp.5, 0
  %v.17 = insertvalue { ptr, i64 } %tmp.8, i64 %tmp.7, 1
  store { ptr, i64 } %v.17, ptr %local.5, align 8
  %v.18 = load ptr, ptr %local.4, align 8
  %v.19 = load ptr, ptr %local.4, align 8
  %v.20 = load i32, ptr %v.19, align 4
  %v.21 = load { ptr, i64 }, ptr %local.5, align 8
  %v.22 = add i32 0, 0
  %tmp.9 = extractvalue { ptr, i64 } %v.21, 0
  %tmp.10 = extractvalue { ptr, i64 } %v.21, 1
  %tmp.11 = sext i32 %v.22 to i64
  %tmp.12 = icmp uge i64 %tmp.11, %tmp.10
  br i1 %tmp.12, label %zil.trap, label %checked.3
checked.3:
  %v.23 = getelementptr i8, ptr %tmp.9, i64 %tmp.11
  %v.24 = getelementptr i8, ptr %v.23, i64 0
  %v.25 = load i8, ptr %v.24, align 1
  %v.26 = zext i8 %v.25 to i32
  %tmp.13 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.20, i32 %v.26)
  %v.27 = extractvalue { i32, i1 } %tmp.13, 0
  %tmp.14 = extractvalue { i32, i1 } %tmp.13, 1
  br i1 %tmp.14, label %zil.trap, label %checked.4
checked.4:
  store i32 %v.27, ptr %v.18, align 4
  %v.28 = load ptr, ptr %local.4, align 8
  %v.29 = load i32, ptr %v.28, align 4
  %v.30 = add i32 0, 42
  %tmp.15 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.29, i32 %v.30)
  %v.31 = extractvalue { i32, i1 } %tmp.15, 0
  %tmp.16 = extractvalue { i32, i1 } %tmp.15, 1
  br i1 %tmp.16, label %zil.trap, label %checked.5
checked.5:
  %tmp.17 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.17, label %package.drop.6, label %package.done.6
package.drop.6:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.18 = getelementptr { { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.18)
  br label %package.done.6
package.done.6:
  store i1 false, ptr %resource.live.3, align 1
  ret i32 %v.31
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
