; ZIL verified LLVM module
; verified-fingerprint: 194c83fc57020b5c874fec446bc802a36e249ec53b4ff3898ddd0dbc291ea19c
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca %zil_env_0, align 1
  %local.1 = alloca { i32, i1 }, align 4
  br label %bb.0
bb.0:
  %v.0 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.0, ptr %local.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = add i32 0, 7
  %v.5 = or i1 false, true
%tmp.0 = insertvalue { i32, i1 } zeroinitializer, i32 %v.4, 0
%v.6 = insertvalue { i32, i1 } %tmp.0, i1 %v.5, 1
  %v.7 = call { i32, i1 } @zil.fn.2(ptr %v.3, { i32, i1 } %v.6)
  store { i32, i1 } %v.7, ptr %local.1, align 4
  %v.8 = load { i32, i1 }, ptr %local.1, align 4
%v.9 = extractvalue { i32, i1 } %v.8, 0
  %v.10 = add i32 0, 7
  %tmp.1 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i32, i1 } @zil.fn.1(%zil_env_0 %env, { i32, i1 } %arg.0) {
entry:
  %local.0 = alloca { i32, i1 }, align 4
  store { i32, i1 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { i32, i1 }, ptr %local.0, align 4
  ret { i32, i1 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i32, i1 } @zil.fn.2(ptr %arg.0, { i32, i1 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { i32, i1 }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store { i32, i1 } %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load { i32, i1 }, ptr %local.1, align 4
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.2 = call { i32, i1 } @zil.fn.1(%zil_env_0 %tmp.0, { i32, i1 } %v.1)
  ret { i32, i1 } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
