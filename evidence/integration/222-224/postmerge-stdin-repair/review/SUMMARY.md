# stdin 驱动修复独立审核

`/root/ct_for` 对精确代码 `e38ce27fda432e7a55b4a203f4fd047756d9512e`、作者交付 `bcce4ca58cbd7edb1692c9140a0babb314491ddc` 裁决 **Pass**，范围仅为原生测试驱动的 stdin 协议。仅测试文件改变；两个 ZIL 源及全部退出、输出、清理和 allocator oracle 不变。

独立全新 target 显式启用 `llvm-api`，原两测试实际 2/0/0（零 filtered），四次 O0/O2 Build 均为真实 LLVM API builder，60 次原生执行包括 2 次无输入和 58 次严格字节输入；50 次 allocator 观察含 34 次真实申请失败，全部 live=0。另行 4 次原生控制按独立计数：O0/O2 各一次人为固定子进程先退出的旧写入，确实 BrokenPipe 且未到输出采集；各一次 None 控制核回 exit=1、stdout=ABCDEF、stderr为空。

原 main CI 41101248493 的 2860/1/0 保留为 Fail；没有声称原 CI 子进程输出已通过。作者首次未启 feature 的零选择记录继续为 NotRun，作者重跑、独立执行与 archive 校验分别记账。本审核不替代 root 最终完整 CI。精确命令、身份、逐项机器收据及归档清单见同目录 JSON。
