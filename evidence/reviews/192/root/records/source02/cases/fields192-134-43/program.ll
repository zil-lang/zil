; ZIL verified LLVM module
; verified-fingerprint: e9f9e30b73a4b5b600203c55f1227e33c1907382a3742c15a3d7c16adb012567
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { { i32 }, i32, i32 }, i1, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %field.flag.0.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
  %field.live.0.0 = load i1, ptr %field.flag.0.0, align 1
  br i1 %field.live.0.0, label %field.drop.0.0, label %field.done.0.0
field.drop.0.0:
  store i1 false, ptr %field.flag.0.0, align 1
  br label %field.done.0.0
field.done.0.0:
  br label %done.0
done.0:
  ret void
}
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
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca %zil_env_0, align 4
  br label %bb.0
bb.0:
  %v.0 = call { { i32 }, i32, i32 } @zil.fn.0()
  store i1 false, ptr %resource.live.0, align 1
  store { { i32 }, i32, i32 } %v.0, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = load { { i32 }, i32, i32 }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, { { i32 }, i32, i32 } %v.1, 0
  %tmp.1 = insertvalue %zil_env_0 %tmp.0, i1 true, 1
  %v.2 = insertvalue %zil_env_0 %tmp.1, i1 true, 2
  store i1 false, ptr %resource.live.1, align 1
  store %zil_env_0 %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = call i32 @zil.fn.2(ptr %local.1)
  %v.4 = call i32 @zil.fn.2(ptr %local.1)
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.4)
  %v.5 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.6 = add i32 0, 43
  %tmp.4 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.5, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.1
checked.1:
  %tmp.6 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.6, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.closure.drop.0(ptr %local.1)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.7 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.7, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.0, align 1
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(ptr %env) {
entry:
  %local.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %resource.live.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %package.live.0.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
  %local.1 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr { { i32 }, i32, i32 }, ptr %local.0, i32 0, i32 1
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  store ptr %v.1, ptr %local.1, align 8
  %v.2 = load ptr, ptr %local.1, align 8
  %v.3 = load ptr, ptr %local.1, align 8
  %v.4 = load i32, ptr %v.3, align 4
  %v.5 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.6, ptr %v.2, align 4
  %v.7 = load ptr, ptr %local.1, align 8
  %v.8 = load i32, ptr %v.7, align 4
  ret i32 %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}
