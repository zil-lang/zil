; ZIL verified LLVM module
; verified-fingerprint: 21e5824fdd182d4123db989089c74d1a01287e26e2d1a0ea4ad10919e03857da
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define { { i32 }, i32 } @zil.fn.0() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = add i32 0, 1
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = add i32 0, 41
  store i32 %v.3, ptr %local.2, align 4
  %v.4 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.4, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.5 = load i32, ptr %local.2, align 4
  %v.6 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
%tmp.0 = insertvalue { { i32 }, i32 } zeroinitializer, { i32 } %v.6, 0
%v.7 = insertvalue { { i32 }, i32 } %tmp.0, i32 %v.5, 1
  %tmp.1 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  ret { { i32 }, i32 } %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, i32 } @zil.fn.1({ { i32 }, i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, i32 }, align 4
  store { { i32 }, i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { { i32 }, i32 }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, i32 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2() {
entry:
  br label %bb.0
bb.0:
  %v.0 = call i32 @zil.fn.3()
  %v.1 = add i32 0, 41
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

define i32 @zil.fn.3() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, i32 }, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, i32 }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.1, align 1
  %v.0 = call { { i32 }, i32 } @zil.fn.0()
  store i1 false, ptr %resource.live.0, align 1
  store { { i32 }, i32 } %v.0, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = load { { i32 }, i32 }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store { { i32 }, i32 } %v.1, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.2 = load { { i32 }, i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %v.3 = call { { i32 }, i32 } @zil.fn.1({ { i32 }, i32 } %v.2)
  store { { i32 }, i32 } %v.3, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  %tmp.0 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  br label %package.done.0
package.done.0:
  %tmp.1 = getelementptr { { i32 }, i32 }, ptr %local.0, i32 0, i32 1
  %v.4 = load i32, ptr %tmp.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %tmp.2 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.2, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.2()
  ret i32 %entry.result
}
