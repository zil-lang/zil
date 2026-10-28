; ZIL verified LLVM module
; verified-fingerprint: 18283078c8dd016dab9b7630f0ac6e9184f2b58185d76be43b708d83b3818c44
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)

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

define { i1, i1, i32 } @zil.fn.2(i1 %arg.0) {
entry:
  %local.0 = alloca i1, align 1
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i32 }, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { { i32 }, i32 }, align 4
  %local.6 = alloca ptr, align 8
  store i1 %arg.0, ptr %local.0, align 1
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.4, align 1
  %v.0 = add i32 0, 65
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.2, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = add i32 0, 40
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
  store i1 false, ptr %resource.live.5, align 1
  store { { i32 }, i32 } %v.7, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.8 = getelementptr { { i32 }, i32 }, ptr %local.5, i32 0, i32 1
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  store ptr %v.9, ptr %local.6, align 8
  %v.10 = load ptr, ptr %local.6, align 8
  %v.11 = load ptr, ptr %local.6, align 8
  %v.12 = load i32, ptr %v.11, align 4
  %v.13 = add i32 0, 2
  %tmp.3 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.12, i32 %v.13)
  %v.14 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.2
checked.2:
  store i32 %v.14, ptr %v.10, align 4
  %v.15 = load i1, ptr %local.0, align 1
  br i1 %v.15, label %bb.1, label %bb.2
bb.1:
  %v.16 = or i1 false, true
  %tmp.5 = insertvalue { i1, i1, i32 } zeroinitializer, i1 true, 0
  %v.17 = insertvalue { i1, i1, i32 } %tmp.5, i1 %v.16, 1
  %tmp.6 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.6, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.7 = getelementptr { { i32 }, i32 }, ptr %local.5, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.7)
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.5, align 1
  ret { i1, i1, i32 } %v.17
bb.2:
  br label %bb.3
bb.3:
  %v.18 = load ptr, ptr %local.6, align 8
  %v.19 = load i32, ptr %v.18, align 4
  %tmp.8 = insertvalue { i1, i1, i32 } zeroinitializer, i1 false, 0
  %v.20 = insertvalue { i1, i1, i32 } %tmp.8, i32 %v.19, 2
  %tmp.9 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.9, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.10 = getelementptr { { i32 }, i32 }, ptr %local.5, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.10)
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.5, align 1
  ret { i1, i1, i32 } %v.20
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3() {
entry:
  %local.0 = alloca { i1, i1, i32 }, align 4
  %local.1 = alloca { i1, i1, i32 }, align 4
  br label %bb.0
bb.0:
  %v.0 = or i1 false, true
  %v.1 = call { i1, i1, i32 } @zil.fn.2(i1 %v.0)
  store { i1, i1, i32 } %v.1, ptr %local.0, align 4
  %v.2 = or i1 false, false
  %v.3 = call { i1, i1, i32 } @zil.fn.2(i1 %v.2)
  store { i1, i1, i32 } %v.3, ptr %local.1, align 4
  %v.4 = add i32 0, 0
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.3()
  ret i32 %entry.result
}
