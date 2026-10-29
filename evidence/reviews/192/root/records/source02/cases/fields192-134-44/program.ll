; ZIL verified LLVM module
; verified-fingerprint: 9c105643ca688eafaf6234b695aae7af5703da76cb71c614a3e76fac30f23a18
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define { { i32 }, i32, i32 } @zil.fn.0() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i32 }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.4, align 1
  %v.0 = add i32 0, 1
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = add i32 0, 20
  store i32 %v.3, ptr %local.2, align 4
  %v.4 = add i32 0, 1
  store i32 %v.4, ptr %local.3, align 4
  %v.5 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.5, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.6 = load i32, ptr %local.2, align 4
  %v.7 = load i32, ptr %local.3, align 4
  %v.8 = load { i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
%tmp.0 = insertvalue { { i32 }, i32, i32 } zeroinitializer, { i32 } %v.8, 0
%tmp.1 = insertvalue { { i32 }, i32, i32 } %tmp.0, i32 %v.6, 1
%v.9 = insertvalue { { i32 }, i32, i32 } %tmp.1, i32 %v.7, 2
  %tmp.2 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.2, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.4, align 1
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.3 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  ret { { i32 }, i32, i32 } %v.9
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, i32, i32 }, align 4
  %local.1 = alloca ptr, align 8
  %local.2 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = call { { i32 }, i32, i32 } @zil.fn.0()
  store i1 false, ptr %resource.live.0, align 1
  store { { i32 }, i32, i32 } %v.0, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = getelementptr { { i32 }, i32, i32 }, ptr %local.0, i32 0, i32 1
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  store ptr %v.2, ptr %local.1, align 8
  %v.3 = getelementptr { { i32 }, i32, i32 }, ptr %local.0, i32 0, i32 1
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  store ptr %v.4, ptr %local.2, align 8
  %v.5 = load ptr, ptr %local.1, align 8
  %v.6 = load i32, ptr %v.5, align 4
  %v.7 = load ptr, ptr %local.2, align 8
  %v.8 = load i32, ptr %v.7, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.6, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %tmp.2 = getelementptr { { i32 }, i32, i32 }, ptr %local.0, i32 0, i32 1
  %v.10 = load i32, ptr %tmp.2, align 4
  %tmp.3 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.12 = add i32 0, 60
  %tmp.5 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.11, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.2
checked.2:
  %tmp.7 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.7, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.0, align 1
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.13
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}
