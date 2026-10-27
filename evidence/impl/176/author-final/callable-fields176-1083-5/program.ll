; ZIL verified LLVM module
; verified-fingerprint: bb86ba6124946c9b756f0fa64f264360ee9a4d1b7c54dfb1ad398c520e7b6ea7
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32 }
%zil_env_1 = type { i32, { i32 }, i32, i1 }
%zil_env_2 = type { %zil_env_1, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.1(ptr %env) {
entry:
  %flag.ptr.1 = getelementptr %zil_env_1, ptr %env, i32 0, i32 3
  %live.1 = load i1, ptr %flag.ptr.1, align 1
  br i1 %live.1, label %drop.1, label %done.1
drop.1:
  store i1 false, ptr %flag.ptr.1, align 1
  %payload.1 = getelementptr %zil_env_1, ptr %env, i32 0, i32 1
  call void @zil.fn.1(ptr %payload.1)
  br label %done.1
done.1:
  ret void
}
define internal void @zil.closure.drop.2(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_2, ptr %env, i32 0, i32 1
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %child.0 = getelementptr %zil_env_2, ptr %env, i32 0, i32 0
  call void @zil.closure.drop.1(ptr %child.0)
  br label %done.0
done.0:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
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

define %zil_env_1 @zil.fn.3(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca i32, align 4
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
  %v.3 = load i32, ptr %local.0, align 4
  store i32 %v.3, ptr %local.3, align 4
  %v.4 = load i32, ptr %local.3, align 4
  %v.5 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.6 = load i32, ptr %local.0, align 4
  %tmp.0 = insertvalue %zil_env_1 zeroinitializer, i32 %v.4, 0
  %tmp.1 = insertvalue %zil_env_1 %tmp.0, { i32 } %v.5, 1
  %tmp.2 = insertvalue %zil_env_1 %tmp.1, i32 %v.6, 2
  %v.7 = insertvalue %zil_env_1 %tmp.2, i1 true, 3
  %tmp.3 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.3, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.2, align 1
  ret %zil_env_1 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_1 @zil.fn.4(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = call %zil_env_1 @zil.fn.3(i32 %v.0)
  ret %zil_env_1 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_2 @zil.fn.5(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca %zil_env_1, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = call %zil_env_1 @zil.fn.3(i32 %v.0)
  store i1 false, ptr %resource.live.1, align 1
  store %zil_env_1 %v.1, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.2 = load %zil_env_1, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %tmp.0 = insertvalue %zil_env_2 zeroinitializer, %zil_env_1 %v.2, 0
  %v.3 = insertvalue %zil_env_2 %tmp.0, i1 true, 1
  %tmp.1 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.1, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.closure.drop.1(ptr %local.1)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret %zil_env_2 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.6() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_2, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, %zil_env_2, i32 }, align 4
  %package.live.1.0 = alloca i1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  %package.live.1.1 = alloca i1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %local.2 = alloca ptr, align 8
  %local.3 = alloca i1, align 1
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca %zil_env_2, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca %zil_env_2, align 4
  %local.6 = alloca ptr, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.0, align 1
  %v.0 = add i32 0, 65
  %v.1 = call %zil_env_2 @zil.fn.5(i32 %v.0)
  store %zil_env_2 %v.1, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  %v.2 = add i32 0, 84
  %v.3 = load %zil_env_2, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  %v.4 = call { { i32 }, %zil_env_2, i32 } @zil.fn.10(%zil_env_2 %v.3, i32 %v.2)
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.2(ptr %local.0)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store { { i32 }, %zil_env_2, i32 } %v.4, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.0, align 1
  store i1 true, ptr %package.live.1.1, align 1
  %v.5 = getelementptr { { i32 }, %zil_env_2, i32 }, ptr %local.1, i32 0, i32 1
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  store ptr %v.6, ptr %local.2, align 8
  %v.7 = load ptr, ptr %local.2, align 8
  %v.8 = call i32 @zil.fn.9(ptr %v.7)
  %v.9 = add i32 0, 66
  %v.10 = icmp ne i32 %v.8, %v.9
  br i1 %v.10, label %bb.1, label %bb.2
bb.1:
  %v.11 = or i1 false, true
  store i1 %v.11, ptr %local.3, align 1
  br label %bb.3
bb.2:
  %v.12 = load ptr, ptr %local.2, align 8
  %v.13 = call i32 @zil.fn.9(ptr %v.12)
  %v.14 = add i32 0, 67
  %v.15 = icmp ne i32 %v.13, %v.14
  store i1 %v.15, ptr %local.3, align 1
  br label %bb.3
bb.3:
  %v.16 = load i1, ptr %local.3, align 1
  br i1 %v.16, label %bb.4, label %bb.5
bb.4:
  %v.17 = add i32 0, 1
  %tmp.1 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.1, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.2 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.2, label %package.field.drop.2, label %package.field.done.2
package.field.drop.2:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.3 = getelementptr { { i32 }, %zil_env_2, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.3)
  br label %package.field.done.2
package.field.done.2:
  %tmp.4 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.4, label %package.field.drop.3, label %package.field.done.3
package.field.drop.3:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.5 = getelementptr { { i32 }, %zil_env_2, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.2(ptr %tmp.5)
  br label %package.field.done.3
package.field.done.3:
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.5, align 1
  ret i32 %v.17
bb.5:
  br label %bb.6
bb.6:
  %tmp.6 = getelementptr { { i32 }, %zil_env_2, i32 }, ptr %local.1, i32 0, i32 1
  %v.18 = load %zil_env_2, ptr %tmp.6, align 4
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %resource.live.4, align 1
  store %zil_env_2 %v.18, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.19 = add i32 0, 66
  %v.20 = call %zil_env_2 @zil.fn.5(i32 %v.19)
  store %zil_env_2 %v.20, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %tmp.7 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.7, label %package.field.drop.4, label %package.field.done.4
package.field.drop.4:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.8 = getelementptr { { i32 }, %zil_env_2, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.2(ptr %tmp.8)
  br label %package.field.done.4
package.field.done.4:
  %v.21 = load %zil_env_2, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
  %tmp.9 = getelementptr { { i32 }, %zil_env_2, i32 }, ptr %local.1, i32 0, i32 1
  store %zil_env_2 %v.21, ptr %tmp.9, align 4
  store i1 true, ptr %package.live.1.1, align 1
  %tmp.10 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.10, label %closure.drop.5, label %closure.done.5
closure.drop.5:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.closure.drop.2(ptr %local.5)
  br label %closure.done.5
closure.done.5:
  %v.22 = getelementptr { { i32 }, %zil_env_2, i32 }, ptr %local.1, i32 0, i32 1
  %v.23 = getelementptr i8, ptr %v.22, i64 0
  store ptr %v.23, ptr %local.6, align 8
  %v.24 = call i32 @zil.fn.9(ptr %local.4)
  %v.25 = load ptr, ptr %local.6, align 8
  %v.26 = call i32 @zil.fn.9(ptr %v.25)
  %tmp.11 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.24, i32 %v.26)
  %v.27 = extractvalue { i32, i1 } %tmp.11, 0
  %tmp.12 = extractvalue { i32, i1 } %tmp.11, 1
  br i1 %tmp.12, label %zil.trap, label %checked.6
checked.6:
  %v.28 = add i32 0, 135
  %tmp.13 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.27, i32 %v.28)
  %v.29 = extractvalue { i32, i1 } %tmp.13, 0
  %tmp.14 = extractvalue { i32, i1 } %tmp.13, 1
  br i1 %tmp.14, label %zil.trap, label %checked.7
checked.7:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.15 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.15, label %closure.drop.8, label %closure.done.8
closure.drop.8:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.2(ptr %local.4)
  br label %closure.done.8
closure.done.8:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.16 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.16, label %package.drop.9, label %package.done.9
package.drop.9:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.17 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.17, label %package.field.drop.10, label %package.field.done.10
package.field.drop.10:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.18 = getelementptr { { i32 }, %zil_env_2, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.18)
  br label %package.field.done.10
package.field.done.10:
  %tmp.19 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.19, label %package.field.drop.11, label %package.field.done.11
package.field.drop.11:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.20 = getelementptr { { i32 }, %zil_env_2, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.2(ptr %tmp.20)
  br label %package.field.done.11
package.field.done.11:
  br label %package.done.9
package.done.9:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.29
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.7(ptr %env) {
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

define i32 @zil.fn.8(ptr %env) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca ptr, align 8
  %local.2 = alloca i32, align 4
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = getelementptr %zil_env_1, ptr %env, i32 0, i32 1
  store ptr %v.1, ptr %local.1, align 8
  %tmp.1 = getelementptr %zil_env_1, ptr %env, i32 0, i32 2
  %v.2 = load i32, ptr %tmp.1, align 4
  store i32 %v.2, ptr %local.2, align 4
  %v.3 = load i32, ptr %local.0, align 4
  %v.4 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.4)
  %v.5 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.5, ptr %local.0, align 4
  %tmp.4 = getelementptr %zil_env_1, ptr %env, i32 0, i32 0
  store i32 %v.5, ptr %tmp.4, align 4
  %v.6 = load i32, ptr %local.0, align 4
  %v.7 = load ptr, ptr %local.1, align 8
  %tmp.5 = getelementptr { i32 }, ptr %v.7, i32 0, i32 0
  %v.8 = load i32, ptr %tmp.5, align 4
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.6, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  %v.10 = load i32, ptr %local.2, align 4
  %tmp.8 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.9(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_2, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %v.2 = call i32 @zil.fn.8(ptr %v.1)
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, %zil_env_2, i32 } @zil.fn.10(%zil_env_2 %arg.0, i32 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_2, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca %zil_env_2, align 4
  %local.5 = alloca i32, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca %zil_env_2, align 4
  store %zil_env_2 %arg.0, ptr %local.0, align 4
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  %v.0 = load i32, ptr %local.1, align 4
  store i32 %v.0, ptr %local.2, align 4
  %v.1 = load i32, ptr %local.2, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.2, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.3 = load %zil_env_2, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.4, align 1
  store %zil_env_2 %v.3, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.4 = add i32 0, 0
  store i32 %v.4, ptr %local.5, align 4
  %v.5 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.5, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.6 = load %zil_env_2, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  store %zil_env_2 %v.6, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %v.7 = load i32, ptr %local.5, align 4
  %v.8 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  %v.9 = load %zil_env_2, ptr %local.7, align 4
  store i1 false, ptr %resource.live.7, align 1
%tmp.0 = insertvalue { { i32 }, %zil_env_2, i32 } zeroinitializer, { i32 } %v.8, 0
%tmp.1 = insertvalue { { i32 }, %zil_env_2, i32 } %tmp.0, %zil_env_2 %v.9, 1
%v.10 = insertvalue { { i32 }, %zil_env_2, i32 } %tmp.1, i32 %v.7, 2
  %tmp.2 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.2, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.closure.drop.2(ptr %local.7)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.3 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.1(ptr %local.6)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.4 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.4, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.2(ptr %local.4)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.5 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.5, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.1(ptr %local.3)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.6 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.6, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.2(ptr %local.0)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, %zil_env_2, i32 } %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.6()
  ret i32 %entry.result
}
