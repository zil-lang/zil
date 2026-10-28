; ZIL verified LLVM module
; verified-fingerprint: 3036e4ced0f40c1e4aa94c05846196673b979ffc8fdcfdd9f9f8a4211c0393c7
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca [0 x i32], align 4
  %local.1 = alloca %zil_env_0, align 1
  br label %bb.0
bb.0:
%v.0 = freeze [0 x i32] zeroinitializer
  store [0 x i32] %v.0, ptr %local.0, align 4
  %v.1 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.1, ptr %local.1, align 1
  %v.2 = load %zil_env_0, ptr %local.1, align 1
  %v.3 = getelementptr i8, ptr %local.0, i64 0
  %tmp.0 = icmp ugt i64 0, 0
  %tmp.1 = icmp ugt i64 0, 0
  %tmp.2 = or i1 %tmp.0, %tmp.1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %tmp.3 = getelementptr i32, ptr %v.3, i64 0
  %tmp.4 = sub i64 0, 0
  %tmp.5 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.3, 0
  %v.4 = insertvalue { ptr, i64 } %tmp.5, i64 %tmp.4, 1
  %tmp.6 = extractvalue { ptr, i64 } %v.4, 0
  %v.5 = insertvalue { ptr, i64 } %v.4, ptr %tmp.6, 0
  %tmp.7 = extractvalue { ptr, i64 } %v.5, 0
  %v.6 = insertvalue { ptr, i64 } %v.5, ptr %tmp.7, 0
  %v.7 = call i32 @zil.fn.1(%zil_env_0 %v.2, { ptr, i64 } %v.6)
  ret i32 %v.7
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
