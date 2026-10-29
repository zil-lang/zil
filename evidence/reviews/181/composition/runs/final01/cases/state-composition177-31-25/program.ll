; ZIL verified LLVM module
; verified-fingerprint: f53944d5524aabb7e6ecb1ef553190b9a78ece8aa9ce03c63d39bccaff17b70e
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
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

define i32 @zil.fn.1() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, i32 }, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca [1 x { { i32 }, i32 }], align 4
  %array.live.1.0 = alloca i1, align 1
  store i1 false, ptr %array.live.1.0, align 1
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %array.live.1.0, align 1
  store i1 false, ptr %resource.live.0, align 1
  %v.0 = call { { i32 }, i32 } @zil.fn.0()
  store { { i32 }, i32 } %v.0, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = load { { i32 }, i32 }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
%v.2 = insertvalue [1 x { { i32 }, i32 }] zeroinitializer, { { i32 }, i32 } %v.1, 0
  store [1 x { { i32 }, i32 }] %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %array.live.1.0, align 1
  %v.3 = load [1 x { { i32 }, i32 }], ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %array.live.1.0, align 1
  %v.4 = call i32 @zil.fn.2([1 x { { i32 }, i32 }] %v.3)
  %v.5 = add i32 0, 82
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %array.drop.1, label %array.done.1
array.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.3 = load i1, ptr %array.live.1.0, align 1
  br i1 %tmp.3, label %array.element.drop.2, label %array.element.done.2
array.element.drop.2:
  store i1 false, ptr %array.live.1.0, align 1
  br label %array.element.done.2
array.element.done.2:
  br label %array.done.1
array.done.1:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %array.live.1.0, align 1
  %tmp.4 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.4, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.0, align 1
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2([1 x { { i32 }, i32 }] %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca [1 x { { i32 }, i32 }], align 4
  %array.live.0.0 = alloca i1, align 1
  store i1 true, ptr %array.live.0.0, align 1
  store [1 x { { i32 }, i32 }] %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr [1 x { { i32 }, i32 }], ptr %local.0, i32 0, i32 0, i32 1
  %v.0 = load i32, ptr %tmp.0, align 4
  %tmp.1 = getelementptr [1 x { { i32 }, i32 }], ptr %local.0, i32 0, i32 0, i32 1
  %v.1 = load i32, ptr %tmp.1, align 4
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.0, i32 %v.1)
  %v.2 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %tmp.4 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.4, label %array.drop.1, label %array.done.1
array.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.5 = load i1, ptr %array.live.0.0, align 1
  br i1 %tmp.5, label %array.element.drop.2, label %array.element.done.2
array.element.drop.2:
  store i1 false, ptr %array.live.0.0, align 1
  br label %array.element.done.2
array.element.done.2:
  br label %array.done.1
array.done.1:
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %array.live.0.0, align 1
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}
