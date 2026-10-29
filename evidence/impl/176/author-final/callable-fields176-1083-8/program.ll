; ZIL verified LLVM module
; verified-fingerprint: ceaf39cf61da0094c73eb9dca7000d12fb47daeb580057062ee5a1996738c0bb
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, %zil_env_0, i32 }, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, %zil_env_0, i32 }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.0, align 1
  %v.0 = add i32 0, 40
  %v.1 = call %zil_env_0 @zil.fn.3(i32 %v.0)
  %v.2 = add i32 0, 65
  %v.3 = call { { i32 }, %zil_env_0, i32 } @zil.fn.5(%zil_env_0 %v.1, i32 %v.2)
  store { { i32 }, %zil_env_0, i32 } %v.3, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  %v.4 = load { { i32 }, %zil_env_0, i32 }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  %v.5 = call { { i32 }, %zil_env_0, i32 } @zil.fn.6({ { i32 }, %zil_env_0, i32 } %v.4)
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.1 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.1)
  %tmp.2 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.2)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.1, align 1
  store { { i32 }, %zil_env_0, i32 } %v.5, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %tmp.3 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 2
  %v.6 = load i32, ptr %tmp.3, align 4
  %v.7 = add i32 0, 42
  %tmp.4 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.6, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.1
checked.1:
  %tmp.6 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.6, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.7 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.7)
  %tmp.8 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.8)
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.2(ptr %arg.0) {
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

define %zil_env_0 @zil.fn.3(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = insertvalue %zil_env_0 zeroinitializer, i32 %v.1, 0
  ret %zil_env_0 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.4(ptr %env) {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = add i32 0, 1
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.1, i32 %v.2)
  %v.3 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.3, ptr %local.0, align 4
  %tmp.3 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store i32 %v.3, ptr %tmp.3, align 4
  %v.4 = load i32, ptr %local.0, align 4
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, %zil_env_0, i32 } @zil.fn.5(%zil_env_0 %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca %zil_env_0, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %local.4 = alloca %zil_env_0, align 4
  %local.5 = alloca i32, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  store %zil_env_0 %arg.0, ptr %local.0, align 4
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.6, align 1
  %v.0 = load i32, ptr %local.1, align 4
  store i32 %v.0, ptr %local.2, align 4
  %v.1 = load i32, ptr %local.2, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.2, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.3 = load %zil_env_0, ptr %local.0, align 4
  store %zil_env_0 %v.3, ptr %local.4, align 4
  %v.4 = add i32 0, 0
  store i32 %v.4, ptr %local.5, align 4
  %v.5 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.5, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.6 = load %zil_env_0, ptr %local.4, align 4
  %v.7 = load i32, ptr %local.5, align 4
  %v.8 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
%tmp.0 = insertvalue { { i32 }, %zil_env_0, i32 } zeroinitializer, { i32 } %v.8, 0
%tmp.1 = insertvalue { { i32 }, %zil_env_0, i32 } %tmp.0, %zil_env_0 %v.6, 1
%v.9 = insertvalue { { i32 }, %zil_env_0, i32 } %tmp.1, i32 %v.7, 2
  %tmp.2 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.2, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.2(ptr %local.6)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.3 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.2(ptr %local.3)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.3, align 1
  ret { { i32 }, %zil_env_0, i32 } %v.9
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, %zil_env_0, i32 } @zil.fn.6({ { i32 }, %zil_env_0, i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, %zil_env_0, i32 }, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, %zil_env_0, i32 }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { i32 }, %zil_env_0, i32 }, align 4
  store { { i32 }, %zil_env_0, i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %v.0 = load { { i32 }, %zil_env_0, i32 }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store { { i32 }, %zil_env_0, i32 } %v.0, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = load { { i32 }, %zil_env_0, i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %v.2 = call { { i32 }, %zil_env_0, i32 } @zil.fn.7({ { i32 }, %zil_env_0, i32 } %v.1)
  store { { i32 }, %zil_env_0, i32 } %v.2, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = load { { i32 }, %zil_env_0, i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.4 = call { { i32 }, %zil_env_0, i32 } @zil.fn.7({ { i32 }, %zil_env_0, i32 } %v.3)
  %tmp.0 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.1 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.1)
  %tmp.2 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.2)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.3 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.3, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.4 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.4)
  %tmp.5 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.5)
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.6 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.6, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.7 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.7)
  %tmp.8 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.8)
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, %zil_env_0, i32 } %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, %zil_env_0, i32 } @zil.fn.7({ { i32 }, %zil_env_0, i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, %zil_env_0, i32 }, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, %zil_env_0, i32 }, align 4
  %package.live.1.0 = alloca i1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  %package.live.1.1 = alloca i1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %local.2 = alloca ptr, align 8
  store { { i32 }, %zil_env_0, i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { { i32 }, %zil_env_0, i32 }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store { { i32 }, %zil_env_0, i32 } %v.0, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.0, align 1
  store i1 true, ptr %package.live.1.1, align 1
  %v.1 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 1
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  store ptr %v.2, ptr %local.2, align 8
  %v.3 = load ptr, ptr %local.2, align 8
  %v.4 = call i32 @zil.fn.4(ptr %v.3)
  %tmp.0 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 2
  store i32 %v.4, ptr %tmp.0, align 4
  %v.5 = load { { i32 }, %zil_env_0, i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.1 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.1, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.2 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.2, label %package.field.drop.1, label %package.field.done.1
package.field.drop.1:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.3 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.3)
  br label %package.field.done.1
package.field.done.1:
  %tmp.4 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.4, label %package.field.drop.2, label %package.field.done.2
package.field.drop.2:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.5 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.5)
  br label %package.field.done.2
package.field.done.2:
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.6 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.6, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.7 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.7)
  %tmp.8 = getelementptr { { i32 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.8)
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, %zil_env_0, i32 } %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
