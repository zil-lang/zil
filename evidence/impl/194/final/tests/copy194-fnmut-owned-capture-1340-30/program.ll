; ZIL verified LLVM module
; verified-fingerprint: ea6b6803d731d11a6765d8f792cfae78d45416cdefd2bf828a1fa913439837ec
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { i32 }, i32, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
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

define i32 @zil.fn.2() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca %zil_env_0, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 65
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = add i32 0, 0
  store i32 %v.3, ptr %local.2, align 4
  %v.4 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %v.5 = load i32, ptr %local.2, align 4
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, { i32 } %v.4, 0
  %tmp.1 = insertvalue %zil_env_0 %tmp.0, i32 %v.5, 1
  %v.6 = insertvalue %zil_env_0 %tmp.1, i1 true, 2
  store i1 false, ptr %resource.live.3, align 1
  store %zil_env_0 %v.6, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.7 = getelementptr i8, ptr %local.3, i64 0
  %v.8 = getelementptr i8, ptr %v.7, i64 0
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = add i32 0, 3
  %v.11 = call i32 @zil.fn.4(ptr %v.9, i32 %v.10)
  %v.12 = getelementptr i8, ptr %local.3, i64 0
  %v.13 = getelementptr i8, ptr %v.12, i64 0
  %v.14 = getelementptr i8, ptr %v.13, i64 0
  %v.15 = add i32 0, 4
  %v.16 = call i32 @zil.fn.4(ptr %v.14, i32 %v.15)
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.11, i32 %v.16)
  %v.17 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.18 = add i32 0, 10
  %tmp.4 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.17, i32 %v.18)
  %v.19 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.1
checked.1:
  %tmp.6 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.6, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.closure.drop.0(ptr %local.3)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.7 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.7, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.1, align 1
  ret i32 %v.19
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3(ptr %env, i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca ptr, align 8
  %local.2 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.1, align 8
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %v.1 = load i32, ptr %tmp.0, align 4
  store i32 %v.1, ptr %local.2, align 4
  %v.2 = load ptr, ptr %local.1, align 8
  %tmp.1 = getelementptr { i32 }, ptr %v.2, i32 0, i32 0
  %v.3 = load i32, ptr %tmp.1, align 4
  %v.4 = load i32, ptr %local.2, align 4
  %v.5 = load i32, ptr %local.0, align 4
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.6, ptr %local.2, align 4
  %tmp.4 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store i32 %v.6, ptr %tmp.4, align 4
  %v.7 = load i32, ptr %local.2, align 4
  ret i32 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.4(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = call i32 @zil.fn.3(ptr %v.0, i32 %v.1)
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.2()
  ret i32 %entry.result
}
