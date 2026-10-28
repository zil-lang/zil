# 148：在现有 Roam 上增加截断组合

此适配器使用仓库原版 `tools/roam/vendor/core.ts` 和 `shrink.ts`，没有修改执行/清理/同性质缩减协议，没有另造队列。Rust Roam 外部依赖离线缺失、联网拉取失败，本轮不声称构建了Rust Roam，也不更改它的协议或Cargo.lock。

四类载体：local、Package、FnMut、FnOnce；reserve/push/truncate/fill通过32个实际被调用的helper（每链8层），经过当前生产效果图路径。对完整字节内容、长度、容量及成功/失败逐步核对独立JS模型，并同时运行基线/候选LLVM O0/O2。每次构建确认 `verification_runs==1`。

所有选择保存为roam/1动作流、生成器版本、实际ZIL源码和二进制hash；不能只存seed。固定前缀保证真缩短、非法增长、负长度，生成后续组合。没有注入OOM；既有原语分配失败验证单列，不在此虚报。

受控错误将正截断长度减一，必须失败于同一 `byte-truncate.complete-state/1` 性质并用原shrink缩减。这是发现链控制，不是自然发现的编译器bug。O7证书篡改和异步场景未纳入本切片。

重放：先用tsc把vendor/core.ts、vendor/shrink.ts编译到新目录（commonjs,ES2022）；再 `node tools/roam/compositions/truncate.cjs --out NEW --core COMPILED_DIR --baseline BASE_ZIL --candidate CANDIDATE_ZIL --seeds 8`。输出目录必须不存在；全部实际源码、命令、stdout/stderr和失败保存。进程各有30秒期限。
