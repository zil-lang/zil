; ZIL verified LLVM module
; verified-fingerprint: 179f25a4d937c17b28a4aae7a6005dce44a22685bbf896a3c170d39fe75b12fb
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
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
  %local.0 = alloca i32, align 4
  %local.1 = alloca %zil_env_0, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 20
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = insertvalue %zil_env_0 zeroinitializer, i32 %v.1, 0
  store %zil_env_0 %v.2, ptr %local.1, align 4
  %v.3 = load %zil_env_0, ptr %local.1, align 4
  %v.4 = call i32 @zil.fn.2(%zil_env_0 %v.3)
  %v.5 = load %zil_env_0, ptr %local.1, align 4
  %v.6 = call i32 @zil.fn.2(%zil_env_0 %v.5)
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.8 = add i32 0, 80
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.7, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.9
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(%zil_env_0 %env) {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, i32, i32 }, align 4
  %local.2 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = extractvalue %zil_env_0 %env, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = call { { i32 }, i32, i32 } @zil.fn.0()
  store i1 false, ptr %resource.live.1, align 1
  store { { i32 }, i32, i32 } %v.1, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.2 = getelementptr { { i32 }, i32, i32 }, ptr %local.1, i32 0, i32 1
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  store ptr %v.3, ptr %local.2, align 8
  %v.4 = load ptr, ptr %local.2, align 8
  %v.5 = load ptr, ptr %local.2, align 8
  %v.6 = load i32, ptr %v.5, align 4
  %v.7 = load i32, ptr %local.0, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.6, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.8, ptr %v.4, align 4
  %v.9 = load ptr, ptr %local.2, align 8
  %v.10 = load i32, ptr %v.9, align 4
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.1, align 1
  ret i32 %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}
