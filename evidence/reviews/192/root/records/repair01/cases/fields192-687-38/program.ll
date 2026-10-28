; ZIL verified LLVM module
; verified-fingerprint: 33790e79e36bdff797449f026d1e3e4c9b21e2ee29f2927714a70fd6011c30aa
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
  %package.live.3.0 = alloca i1, align 1
  store i1 false, ptr %package.live.3.0, align 1
  %local.4 = alloca ptr, align 8
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { ptr, i64, i64 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = add i32 0, 42
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
  store i1 false, ptr %package.live.3.0, align 1
  store { { ptr, i64, i64 }, i32 } %v.5, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  store i1 true, ptr %package.live.3.0, align 1
  %v.6 = getelementptr { { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 1
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  store ptr %v.7, ptr %local.4, align 8
  %tmp.3 = getelementptr { { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  %v.8 = load { ptr, i64, i64 }, ptr %tmp.3, align 8
  store i1 false, ptr %package.live.3.0, align 1
  store i1 false, ptr %resource.live.5, align 1
  store { ptr, i64, i64 } %v.8, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.9 = load ptr, ptr %local.4, align 8
  %v.10 = load i32, ptr %v.9, align 4
  %v.11 = getelementptr i8, ptr %local.5, i64 0
  %tmp.4 = getelementptr { ptr, i64, i64 }, ptr %v.11, i32 0, i32 1
  %v.12 = load i64, ptr %tmp.4, align 8
  %tmp.5 = icmp slt i64 %v.12, -2147483648
  %tmp.6 = icmp sgt i64 %v.12, 2147483647
  %tmp.7 = or i1 %tmp.5, %tmp.6
  br i1 %tmp.7, label %zil.trap, label %checked.2
checked.2:
  %v.13 = trunc i64 %v.12 to i32
  %tmp.8 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.10, i32 %v.13)
  %v.14 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.3
checked.3:
  %v.15 = add i32 0, 42
  %tmp.10 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.14, i32 %v.15)
  %v.16 = extractvalue { i32, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i32, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.4
checked.4:
  %tmp.12 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.12, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.bytes.drop(ptr %local.5)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.13 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.13, label %package.drop.6, label %package.done.6
package.drop.6:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.14 = load i1, ptr %package.live.3.0, align 1
  br i1 %tmp.14, label %package.field.drop.7, label %package.field.done.7
package.field.drop.7:
  store i1 false, ptr %package.live.3.0, align 1
  %tmp.15 = getelementptr { { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.15)
  br label %package.field.done.7
package.field.done.7:
  br label %package.done.6
package.done.6:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %package.live.3.0, align 1
  ret i32 %v.16
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
