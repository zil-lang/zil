; ZIL verified LLVM module
; verified-fingerprint: c1102f6236cd436649f5dd36ea77b95d45654aa7e2923b416df44cf105c36a8d
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca [2 x i32], align 4
  %local.1 = alloca %zil_env_0, align 1
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 1
  %v.1 = add i32 0, 2
%tmp.0 = insertvalue [2 x i32] zeroinitializer, i32 %v.0, 0
%v.2 = insertvalue [2 x i32] %tmp.0, i32 %v.1, 1
  store [2 x i32] %v.2, ptr %local.0, align 4
  %v.3 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.3, ptr %local.1, align 1
  %v.4 = load %zil_env_0, ptr %local.1, align 1
  %v.5 = getelementptr i8, ptr %local.0, i64 0
  %v.6 = add i32 0, 2
  %v.7 = add i32 0, 1
  %tmp.1 = sext i32 %v.6 to i64
  %tmp.2 = sext i32 %v.7 to i64
  %tmp.3 = icmp ugt i64 %tmp.1, %tmp.2
  %tmp.4 = icmp ugt i64 %tmp.2, 2
  %tmp.5 = or i1 %tmp.3, %tmp.4
  br i1 %tmp.5, label %zil.trap, label %checked.0
checked.0:
  %tmp.6 = getelementptr i32, ptr %v.5, i64 %tmp.1
  %tmp.7 = sub i64 %tmp.2, %tmp.1
  %tmp.8 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.6, 0
  %v.8 = insertvalue { ptr, i64 } %tmp.8, i64 %tmp.7, 1
  %tmp.9 = extractvalue { ptr, i64 } %v.8, 0
  %v.9 = insertvalue { ptr, i64 } %v.8, ptr %tmp.9, 0
  %tmp.10 = extractvalue { ptr, i64 } %v.9, 0
  %v.10 = insertvalue { ptr, i64 } %v.9, ptr %tmp.10, 0
  %v.11 = call i32 @zil.fn.1(%zil_env_0 %v.4, { ptr, i64 } %v.10)
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1(%zil_env_0 %env, { ptr, i64 } %arg.0) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { ptr, i64 }, ptr %local.0, align 8
  %v.1 = add i32 0, 0
  %tmp.0 = extractvalue { ptr, i64 } %v.0, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.0, 1
  %tmp.2 = sext i32 %v.1 to i64
  %tmp.3 = icmp uge i64 %tmp.2, %tmp.1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.2 = getelementptr i32, ptr %tmp.0, i64 %tmp.2
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = load i32, ptr %v.3, align 4
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}
