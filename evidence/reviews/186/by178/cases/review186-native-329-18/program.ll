; ZIL verified LLVM module
; verified-fingerprint: bccb78b4788151ed851e19adbcf77e487e4fc3462914b4f589d8127dfa9c40f4
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

define { ptr, i64, i64 } @zil.fn.0({ i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { i32 }, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca [2 x i32], align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { ptr, i64, i64 }, align 8
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i32 }, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { ptr, i64, i64 }, align 8
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { i32, { i32 }, [2 x i32], { ptr, i64, i64 } }, align 8
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { i32, { i32 }, [2 x i32], { ptr, i64, i64 } }, align 8
  %package.live.8.1 = alloca i1, align 1
  store i1 false, ptr %package.live.8.1, align 1
  %package.live.8.3 = alloca i1, align 1
  store i1 false, ptr %package.live.8.3, align 1
  store { i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.5, align 1
  %v.0 = add i32 0, 7
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load { i32 }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.1, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.2 = add i32 0, 11
  %v.3 = add i32 0, 13
%tmp.0 = insertvalue [2 x i32] zeroinitializer, i32 %v.2, 0
%v.4 = insertvalue [2 x i32] %tmp.0, i32 %v.3, 1
  store [2 x i32] %v.4, ptr %local.3, align 4
  %v.5 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.5, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.6 = load i32, ptr %local.1, align 4
  %v.7 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.7, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.8 = load [2 x i32], ptr %local.3, align 4
  %v.9 = load { ptr, i64, i64 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.9, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  %v.10 = load { i32 }, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
  %v.11 = load { ptr, i64, i64 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
%tmp.1 = insertvalue { i32, { i32 }, [2 x i32], { ptr, i64, i64 } } zeroinitializer, i32 %v.6, 0
%tmp.2 = insertvalue { i32, { i32 }, [2 x i32], { ptr, i64, i64 } } %tmp.1, { i32 } %v.10, 1
%tmp.3 = insertvalue { i32, { i32 }, [2 x i32], { ptr, i64, i64 } } %tmp.2, [2 x i32] %v.8, 2
%v.12 = insertvalue { i32, { i32 }, [2 x i32], { ptr, i64, i64 } } %tmp.3, { ptr, i64, i64 } %v.11, 3
  %tmp.4 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.4, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.bytes.drop(ptr %local.6)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.5 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.5, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.5, align 1
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.6 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.6, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.bytes.drop(ptr %local.4)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.7 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.7, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.2, align 1
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.7, align 1
  store { i32, { i32 }, [2 x i32], { ptr, i64, i64 } } %v.12, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  %v.13 = load { i32, { i32 }, [2 x i32], { ptr, i64, i64 } }, ptr %local.7, align 8
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %package.live.8.1, align 1
  store i1 false, ptr %package.live.8.3, align 1
  store { i32, { i32 }, [2 x i32], { ptr, i64, i64 } } %v.13, ptr %local.8, align 8
  store i1 true, ptr %resource.live.8, align 1
  store i1 true, ptr %package.live.8.1, align 1
  store i1 true, ptr %package.live.8.3, align 1
  %tmp.8 = getelementptr { i32, { i32 }, [2 x i32], { ptr, i64, i64 } }, ptr %local.8, i32 0, i32 3
  %v.14 = load { ptr, i64, i64 }, ptr %tmp.8, align 8
  store i1 false, ptr %package.live.8.3, align 1
  %tmp.9 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.9, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.10 = load i1, ptr %package.live.8.1, align 1
  br i1 %tmp.10, label %package.field.drop.5, label %package.field.done.5
package.field.drop.5:
  store i1 false, ptr %package.live.8.1, align 1
  br label %package.field.done.5
package.field.done.5:
  %tmp.11 = load i1, ptr %package.live.8.3, align 1
  br i1 %tmp.11, label %package.field.drop.6, label %package.field.done.6
package.field.drop.6:
  store i1 false, ptr %package.live.8.3, align 1
  %tmp.12 = getelementptr { i32, { i32 }, [2 x i32], { ptr, i64, i64 } }, ptr %local.8, i32 0, i32 3
  call void @zil.bytes.drop(ptr %tmp.12)
  br label %package.field.done.6
package.field.done.6:
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %package.live.8.1, align 1
  store i1 false, ptr %package.live.8.3, align 1
  %tmp.13 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.13, label %package.drop.7, label %package.done.7
package.drop.7:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.14 = getelementptr { i32, { i32 }, [2 x i32], { ptr, i64, i64 } }, ptr %local.7, i32 0, i32 3
  call void @zil.bytes.drop(ptr %tmp.14)
  br label %package.done.7
package.done.7:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.15 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.15, label %resource.drop.8, label %resource.done.8
resource.drop.8:
  store i1 false, ptr %resource.live.0, align 1
  br label %resource.done.8
resource.done.8:
  store i1 false, ptr %resource.live.0, align 1
  ret { ptr, i64, i64 } %v.14
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1() {
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
  store i1 false, ptr %resource.live.1, align 1
  %v.0 = add i32 0, 17
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %v.4 = call { ptr, i64, i64 } @zil.fn.0({ i32 } %v.3)
  %tmp.0 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.0, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.2, align 1
  store { ptr, i64, i64 } %v.4, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.5 = getelementptr i8, ptr %local.2, i64 0
  %tmp.1 = getelementptr { ptr, i64, i64 }, ptr %v.5, i32 0, i32 1
  %v.6 = load i64, ptr %tmp.1, align 8
  %tmp.2 = icmp slt i64 %v.6, -2147483648
  %tmp.3 = icmp sgt i64 %v.6, 2147483647
  %tmp.4 = or i1 %tmp.2, %tmp.3
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.7 = trunc i64 %v.6 to i32
  %tmp.5 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.5, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.bytes.drop(ptr %local.2)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  ret i32 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}
