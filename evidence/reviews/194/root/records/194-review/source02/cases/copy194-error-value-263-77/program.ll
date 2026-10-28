; ZIL verified LLVM module
; verified-fingerprint: 88fb2babfb7d1e37cfec743058e6e616e7bddd893dd3cf09a1bf92799962485c
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define { i1, i1, i32 } @zil.fn.0() {
entry:
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 7
  %tmp.0 = insertvalue { i1, i1, i32 } zeroinitializer, i1 false, 0
  %v.1 = insertvalue { i1, i1, i32 } %tmp.0, i32 %v.0, 2
  ret { i1, i1, i32 } %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, i1, i32 } @zil.fn.1() {
entry:
  %local.0 = alloca %zil_env_0, align 1
  %local.1 = alloca { i1, i1, i32 }, align 4
  br label %bb.0
bb.0:
  %v.0 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.0, ptr %local.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = call { i1, i1, i32 } @zil.fn.0()
  %v.5 = call { i1, i1, i32 } @zil.fn.3(ptr %v.3, { i1, i1, i32 } %v.4)
  store { i1, i1, i32 } %v.5, ptr %local.1, align 4
  %v.6 = load { i1, i1, i32 }, ptr %local.1, align 4
  %v.7 = extractvalue { i1, i1, i32 } %v.6, 0
  br i1 %v.7, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = extractvalue { i1, i1, i32 } %v.6, 0
  %tmp.1 = icmp ne i1 %tmp.0, true
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.8 = extractvalue { i1, i1, i32 } %v.6, 1
  %tmp.2 = insertvalue { i1, i1, i32 } zeroinitializer, i1 true, 0
  %v.9 = insertvalue { i1, i1, i32 } %tmp.2, i1 %v.8, 1
  ret { i1, i1, i32 } %v.9
bb.2:
  %tmp.3 = extractvalue { i1, i1, i32 } %v.6, 0
  %tmp.4 = icmp ne i1 %tmp.3, false
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.10 = extractvalue { i1, i1, i32 } %v.6, 2
  %v.11 = add i32 0, 7
  %tmp.5 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.10, i32 %v.11)
  %v.12 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.2
checked.2:
  %tmp.7 = insertvalue { i1, i1, i32 } zeroinitializer, i1 false, 0
  %v.13 = insertvalue { i1, i1, i32 } %tmp.7, i32 %v.12, 2
  ret { i1, i1, i32 } %v.13
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, i1, i32 } @zil.fn.2(%zil_env_0 %env, { i1, i1, i32 } %arg.0) {
entry:
  %local.0 = alloca { i1, i1, i32 }, align 4
  store { i1, i1, i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { i1, i1, i32 }, ptr %local.0, align 4
  ret { i1, i1, i32 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, i1, i32 } @zil.fn.3(ptr %arg.0, { i1, i1, i32 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { i1, i1, i32 }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store { i1, i1, i32 } %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load { i1, i1, i32 }, ptr %local.1, align 4
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.2 = call { i1, i1, i32 } @zil.fn.2(%zil_env_0 %tmp.0, { i1, i1, i32 } %v.1)
  ret { i1, i1, i32 } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, i1, i32 } @zil.fn.1()
  %entry.error = extractvalue { i1, i1, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, i1, i32 } %entry.result, 2
  ret i32 %entry.success
}
