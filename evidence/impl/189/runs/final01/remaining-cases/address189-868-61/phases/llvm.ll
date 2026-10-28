; ZIL verified LLVM module
; verified-fingerprint: 1e8b758f987f584d3fbac644473ce7340e1e89ef05ef80ea009bb85d23f3c6c7
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { ptr }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define void @zil.fn.0({ i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { i32 }, align 4
  store { i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca ptr, align 8
  %local.3 = alloca i32, align 4
  %local.4 = alloca ptr, align 8
  %local.5 = alloca %zil_env_0, align 8
  %local.6 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 42
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = getelementptr i8, ptr %local.1, i64 0
  store ptr %v.3, ptr %local.2, align 8
  %v.4 = add i32 0, 42
  store i32 %v.4, ptr %local.3, align 4
  %v.5 = getelementptr i8, ptr %local.3, i64 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  store ptr %v.6, ptr %local.4, align 8
  %v.7 = load ptr, ptr %local.2, align 8
  %v.8 = insertvalue %zil_env_0 zeroinitializer, ptr %v.7, 0
  store %zil_env_0 %v.8, ptr %local.5, align 8
  %v.9 = getelementptr i8, ptr %local.5, i64 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  store ptr %v.10, ptr %local.6, align 8
  %v.11 = load ptr, ptr %local.6, align 8
  %v.12 = load ptr, ptr %local.4, align 8
  %v.13 = getelementptr i8, ptr %v.12, i64 0
  %tmp.0 = load %zil_env_0, ptr %v.11, align 8
  %v.14 = call i32 @zil.fn.2(%zil_env_0 %tmp.0, ptr %v.13)
  %v.15 = add i32 0, 42
  %tmp.1 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.14, i32 %v.15)
  %v.16 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %tmp.3 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  ret i32 %v.16
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(%zil_env_0 %env, ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = extractvalue %zil_env_0 %env, 0
  store ptr %v.0, ptr %local.1, align 8
  %v.1 = load ptr, ptr %local.1, align 8
  %v.2 = add i32 0, 43
  %tmp.0 = getelementptr { i32 }, ptr %v.1, i32 0, i32 0
  store i32 %v.2, ptr %tmp.0, align 4
  %v.3 = load ptr, ptr %local.0, align 8
  %v.4 = load i32, ptr %v.3, align 4
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}
