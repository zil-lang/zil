; ZIL verified LLVM module
; verified-fingerprint: 40cc75a023904f04305df39bffd0fffb7bc6149008bf5695e8dd5647e84000b9
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { [2 x i32] }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define %zil_env_0 @zil.fn.0() {
entry:
  %local.0 = alloca [2 x i32], align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 7
  %v.1 = add i32 0, 11
%tmp.0 = insertvalue [2 x i32] zeroinitializer, i32 %v.0, 0
%v.2 = insertvalue [2 x i32] %tmp.0, i32 %v.1, 1
  store [2 x i32] %v.2, ptr %local.0, align 4
  %v.3 = load [2 x i32], ptr %local.0, align 4
  %v.4 = insertvalue %zil_env_0 zeroinitializer, [2 x i32] %v.3, 0
  ret %zil_env_0 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1() {
entry:
  %local.0 = alloca %zil_env_0, align 4
  %local.1 = alloca { ptr, i64 }, align 8
  br label %bb.0
bb.0:
  %v.0 = call %zil_env_0 @zil.fn.0()
  store %zil_env_0 %v.0, ptr %local.0, align 4
  %v.1 = call { ptr, i64 } @zil.fn.2(ptr %local.0)
  store { ptr, i64 } %v.1, ptr %local.1, align 8
  %v.2 = load { ptr, i64 }, ptr %local.1, align 8
  %v.3 = add i32 0, 0
  %tmp.0 = extractvalue { ptr, i64 } %v.2, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.2, 1
  %tmp.2 = sext i32 %v.3 to i64
  %tmp.3 = icmp uge i64 %tmp.2, %tmp.1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.4 = getelementptr i32, ptr %tmp.0, i64 %tmp.2
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = load i32, ptr %v.5, align 4
  %v.7 = load { ptr, i64 }, ptr %local.1, align 8
  %v.8 = add i32 0, 1
  %tmp.4 = extractvalue { ptr, i64 } %v.7, 0
  %tmp.5 = extractvalue { ptr, i64 } %v.7, 1
  %tmp.6 = sext i32 %v.8 to i64
  %tmp.7 = icmp uge i64 %tmp.6, %tmp.5
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  %v.9 = getelementptr i32, ptr %tmp.4, i64 %tmp.6
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = load i32, ptr %v.10, align 4
  %tmp.8 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.6, i32 %v.11)
  %v.12 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  %v.13 = load { ptr, i64 }, ptr %local.1, align 8
  %v.14 = extractvalue { ptr, i64 } %v.13, 1
  %tmp.10 = icmp slt i64 %v.14, -2147483648
  %tmp.11 = icmp sgt i64 %v.14, 2147483647
  %tmp.12 = or i1 %tmp.10, %tmp.11
  br i1 %tmp.12, label %zil.trap, label %checked.3
checked.3:
  %v.15 = trunc i64 %v.14 to i32
  %tmp.13 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.12, i32 %v.15)
  %v.16 = extractvalue { i32, i1 } %tmp.13, 0
  %tmp.14 = extractvalue { i32, i1 } %tmp.13, 1
  br i1 %tmp.14, label %zil.trap, label %checked.4
checked.4:
  %v.17 = add i32 0, 20
  %tmp.15 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.16, i32 %v.17)
  %v.18 = extractvalue { i32, i1 } %tmp.15, 0
  %tmp.16 = extractvalue { i32, i1 } %tmp.15, 1
  br i1 %tmp.16, label %zil.trap, label %checked.5
checked.5:
  ret i32 %v.18
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64 } @zil.fn.2(ptr %env) {
entry:
  %local.0 = alloca [2 x i32], align 4
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load [2 x i32], ptr %tmp.0, align 4
  store [2 x i32] %v.0, ptr %local.0, align 4
  %v.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %tmp.1 = icmp ugt i64 0, 2
  %tmp.2 = icmp ugt i64 2, 2
  %tmp.3 = or i1 %tmp.1, %tmp.2
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %tmp.4 = getelementptr i32, ptr %v.1, i64 0
  %tmp.5 = sub i64 2, 0
  %tmp.6 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.4, 0
  %v.2 = insertvalue { ptr, i64 } %tmp.6, i64 %tmp.5, 1
  %tmp.7 = extractvalue { ptr, i64 } %v.2, 0
  %v.3 = insertvalue { ptr, i64 } %v.2, ptr %tmp.7, 0
  ret { ptr, i64 } %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}
