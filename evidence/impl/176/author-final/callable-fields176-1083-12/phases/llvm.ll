; ZIL verified LLVM module
; verified-fingerprint: 40640e34cdf39d050f12df6141dd2a1238f39bb67cd1bea2576934248f759cf9
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { i32 }, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  call void @zil.fn.1(ptr %payload.0)
  br label %done.0
done.0:
  ret void
}
declare void @llvm.trap()
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

define %zil_env_0 @zil.fn.2(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.2, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, { i32 } %v.3, 0
  %v.4 = insertvalue %zil_env_0 %tmp.0, i1 true, 1
  %tmp.1 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.2, align 1
  ret %zil_env_0 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, %zil_env_0 }, align 4
  %package.live.1.0 = alloca i1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  %package.live.1.1 = alloca i1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_0, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca %zil_env_0, align 4
  %closure.once.6 = alloca %zil_env_0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.0, align 1
  %v.0 = add i32 0, 65
  %v.1 = call %zil_env_0 @zil.fn.2(i32 %v.0)
  store %zil_env_0 %v.1, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  %v.2 = load %zil_env_0, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  %v.3 = call { { i32 }, %zil_env_0 } @zil.fn.5(%zil_env_0 %v.2)
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store { { i32 }, %zil_env_0 } %v.3, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.0, align 1
  store i1 true, ptr %package.live.1.1, align 1
  %tmp.1 = getelementptr { { i32 }, %zil_env_0 }, ptr %local.1, i32 0, i32 1
  %v.4 = load %zil_env_0, ptr %tmp.1, align 4
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.4, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.5 = load %zil_env_0, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.5, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.6 = load %zil_env_0, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store %zil_env_0 %v.6, ptr %closure.once.6, align 4
  %v.7 = call i32 @zil.fn.4(ptr %closure.once.6)
  call void @zil.closure.drop.0(ptr %closure.once.6)
  %v.8 = add i32 0, 65
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.7, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  %tmp.4 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.4, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.closure.drop.0(ptr %local.3)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.5 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.5, label %closure.drop.3, label %closure.done.3
closure.drop.3:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.3
closure.done.3:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.6 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.6, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.7 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.7, label %package.field.drop.5, label %package.field.done.5
package.field.drop.5:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.8 = getelementptr { { i32 }, %zil_env_0 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.8)
  br label %package.field.done.5
package.field.done.5:
  %tmp.9 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.9, label %package.field.drop.6, label %package.field.done.6
package.field.drop.6:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.10 = getelementptr { { i32 }, %zil_env_0 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.10)
  br label %package.field.done.6
package.field.done.6:
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.9
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.4(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store i1 false, ptr %tmp.0, align 1
  %tmp.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.1 = load { i32 }, ptr %tmp.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.1, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.2 = getelementptr i8, ptr %local.1, i64 0
  %tmp.2 = getelementptr { i32 }, ptr %v.2, i32 0, i32 0
  %v.3 = load i32, ptr %tmp.2, align 4
  %tmp.3 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.3, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, %zil_env_0 } @zil.fn.5(%zil_env_0 %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca %zil_env_0, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i32 }, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca %zil_env_0, align 4
  store %zil_env_0 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  %v.0 = add i32 0, 84
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.2, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = load %zil_env_0, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.3, align 1
  store %zil_env_0 %v.3, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.4 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.4, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.5 = load %zil_env_0, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store %zil_env_0 %v.5, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.6 = load { i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  %v.7 = load %zil_env_0, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
%tmp.0 = insertvalue { { i32 }, %zil_env_0 } zeroinitializer, { i32 } %v.6, 0
%v.8 = insertvalue { { i32 }, %zil_env_0 } %tmp.0, %zil_env_0 %v.7, 1
  %tmp.1 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.1, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.closure.drop.0(ptr %local.5)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.2 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.fn.1(ptr %local.4)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.3 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.3, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.closure.drop.0(ptr %local.3)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.4 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.4, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.5 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.5, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, %zil_env_0 } %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.3()
  ret i32 %entry.result
}
