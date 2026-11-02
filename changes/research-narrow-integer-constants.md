# LLVM C API 窄整数常量规范化修复

基线：`646099a70105b6da3821ce7d8ea20bd8e28fe3e1`。编译效率专项的真实混合源码对照发现：`examples/mainline/byte_io/main.zil` 在 LLVM API O2 返回2，O0和clang文本路径返回0，输出字节相同；原专项基线 `5d7c1eeb` 也有此问题。

最小源码含 `if x<0 || x>64 {return -2;} while ...; return 0;`。单独 `ipsccp` 即可使所有返回路径错误地合为-2。Builder调用 `LLVMConstInt(i32, (-2i128) as u64, 0)` 给出的64位值在LLVM 23.1.4中没有被规范化到32位；`LLVMConstIntGetZExtValue` 可见高位仍全1，而打印和bitcode只保留低32位。重新解析文本/bitcode可掩盖该状态，clone不能修复。

修正只在经过machine preflight的整数常量适配处，把位模式显式截到目标1/8/16/32/64位，再调用LLVMConstInt。它保留文本IR约定的模2^width位模式，不增加接受性、改ABI、放松预算或关闭任何优化/Verify。负号语义、无符号最大值和i64均保留。公开schema不变。

验证：所有支持位宽的signed/unsigned同位模式以及相邻不等控制；实际源码负分支/循环、byte_io的NUL/高字节/长输入；O0/O2与LLVM API/clang、冷/暖对象缓存；独立审核精确提交。对象缓存命名空间已绑定编译器ELF身份，新编译器不会复用旧错误对象，另作跨二进制控制确认。

旧基线O2错误产物不能与修正版本作“语义等价提速”样本。完整性能证据分别标记修正前候选与最终候选，不能把往返/跳过ipsccp作为默认快路径。
