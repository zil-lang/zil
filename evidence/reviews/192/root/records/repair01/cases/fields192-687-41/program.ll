; ZIL verified LLVM module
; verified-fingerprint: 8dd6efa564f876c735728df2e212235929818e469499761a223e4ed593b013f5
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
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

define i32 @zil.fn.2() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca i64, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { { i32 }, i64 }, align 8
  %package.live.4.0 = alloca i1, align 1
  store i1 false, ptr %package.live.4.0, align 1
  %local.5 = alloca ptr, align 8
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %local.7 = alloca i32, align 4
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { i32 }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = add i32 0, 65
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = add i64 0, 2
  store i64 %v.3, ptr %local.2, align 8
  %v.4 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.4, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.5 = load i64, ptr %local.2, align 8
  %v.6 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
%tmp.0 = insertvalue { { i32 }, i64 } zeroinitializer, { i32 } %v.6, 0
%v.7 = insertvalue { { i32 }, i64 } %tmp.0, i64 %v.5, 1
  %tmp.1 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.1(ptr %local.3)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %package.live.4.0, align 1
  store { { i32 }, i64 } %v.7, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  store i1 true, ptr %package.live.4.0, align 1
  %v.8 = getelementptr { { i32 }, i64 }, ptr %local.4, i32 0, i32 1
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  store ptr %v.9, ptr %local.5, align 8
  %v.10 = or i1 false, true
  br i1 %v.10, label %bb.1, label %bb.2
bb.1:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.3 = getelementptr { { i32 }, i64 }, ptr %local.4, i32 0, i32 0
  %v.11 = load { i32 }, ptr %tmp.3, align 4
  store i1 false, ptr %package.live.4.0, align 1
  store i1 false, ptr %resource.live.6, align 1
  store { i32 } %v.11, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.12 = add i32 0, 66
  store i32 %v.12, ptr %local.7, align 4
  %v.13 = load i32, ptr %local.7, align 4
%v.14 = insertvalue { i32 } zeroinitializer, i32 %v.13, 0
  store { i32 } %v.14, ptr %local.8, align 4
  store i1 true, ptr %resource.live.8, align 1
  %tmp.4 = load i1, ptr %package.live.4.0, align 1
  br i1 %tmp.4, label %package.field.drop.2, label %package.field.done.2
package.field.drop.2:
  store i1 false, ptr %package.live.4.0, align 1
  %tmp.5 = getelementptr { { i32 }, i64 }, ptr %local.4, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.5)
  br label %package.field.done.2
package.field.done.2:
  %v.15 = load { i32 }, ptr %local.8, align 4
  store i1 false, ptr %resource.live.8, align 1
  %tmp.6 = getelementptr { { i32 }, i64 }, ptr %local.4, i32 0, i32 0
  store { i32 } %v.15, ptr %tmp.6, align 4
  store i1 true, ptr %package.live.4.0, align 1
  %tmp.7 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.7, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.fn.1(ptr %local.8)
  br label %resource.done.3
resource.done.3:
  %v.16 = load ptr, ptr %local.5, align 8
  %v.17 = load ptr, ptr %local.5, align 8
  %v.18 = load i64, ptr %v.17, align 8
  %v.19 = getelementptr i8, ptr %local.6, i64 0
  %tmp.8 = getelementptr { i32 }, ptr %v.19, i32 0, i32 0
  %v.20 = load i32, ptr %tmp.8, align 4
  %v.21 = sext i32 %v.20 to i64
  %tmp.9 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.18, i64 %v.21)
  %v.22 = extractvalue { i64, i1 } %tmp.9, 0
  %tmp.10 = extractvalue { i64, i1 } %tmp.9, 1
  br i1 %tmp.10, label %zil.trap, label %checked.4
checked.4:
  store i64 %v.22, ptr %v.16, align 8
  store i1 false, ptr %resource.live.8, align 1
  %tmp.11 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.11, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.1(ptr %local.6)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.6, align 1
  br label %bb.3
bb.2:
  br label %bb.3
bb.3:
  %v.23 = load ptr, ptr %local.5, align 8
  %v.24 = load i64, ptr %v.23, align 8
  %tmp.12 = icmp slt i64 %v.24, -2147483648
  %tmp.13 = icmp sgt i64 %v.24, 2147483647
  %tmp.14 = or i1 %tmp.12, %tmp.13
  br i1 %tmp.14, label %zil.trap, label %checked.6
checked.6:
  %v.25 = trunc i64 %v.24 to i32
  %v.26 = add i32 0, 67
  %tmp.15 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.25, i32 %v.26)
  %v.27 = extractvalue { i32, i1 } %tmp.15, 0
  %tmp.16 = extractvalue { i32, i1 } %tmp.15, 1
  br i1 %tmp.16, label %zil.trap, label %checked.7
checked.7:
  %tmp.17 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.17, label %package.drop.8, label %package.done.8
package.drop.8:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.18 = load i1, ptr %package.live.4.0, align 1
  br i1 %tmp.18, label %package.field.drop.9, label %package.field.done.9
package.field.drop.9:
  store i1 false, ptr %package.live.4.0, align 1
  %tmp.19 = getelementptr { { i32 }, i64 }, ptr %local.4, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.19)
  br label %package.field.done.9
package.field.done.9:
  br label %package.done.8
package.done.8:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %package.live.4.0, align 1
  ret i32 %v.27
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.2()
  ret i32 %entry.result
}
