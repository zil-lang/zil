# G4.4.6.3 qualified Array Lower 接续

root 明确派工，父合同 G4.4/G4.4.5.4/G4.4.6.3；分支 `task/G4.4.6.3-lower`、树 `/workspace/zil-G4.4.6.3-lower`，基线 `f47e1bb5b`，实施者 `/root/g44_complete`。只接实际 Verify1→Lower Unsupported 新边界，不重做 source producer/三核；原 scope/实际 AST 认证保留。

规范/预算沿本基线具名设计与 CT6/qualified pure Enum/Array 后继合同，runtime Wire 不变。写集为 `crates/zil-lower/src/lib.rs`、`owned_sums.rs` 的纯表示兜底和限定私有测试，必要 LLVM 同级拒绝点另列；root 负责独审/正式门/工程合同和全量，不动共享树、不重设默认限额。

先 changes/G4.4.6.3-qualified-lower.md；保持旧 ordinary/stored/owned 全部成功快路及原计费，再补 slots 空 pure composite 独立计量。新检查不能授 Address/Managed/loan；完整 Type 不替换成 storage。查全部返回、参数、Local/SSA门及 LLVM representation/snapshot/ABI/Array-index。仅 -j1 no-link；限定 linked/native 待 root 唯一窗口。真实三个 Source/native case 与所有未测状态分别记录，不能以 synthetic 或编译通过标机器结果成功。

阶段实现 `b24edbc03ae66751533a02006a6822a7049dde3b`：只改 Lower 的三类门与 Budget 私有 pure composite。spec hash `e235b19010325a125338f764f9e75b8ab6dd612f75ea4915e4761edd4a7e0b7a`，normative hash `e938daf9f797c6790549c578e25d474d6eafeba20bf50ee51b282fbaa653f80b`。正确命令 `cargo check -p zil-lower -p zil-backend-llvm --tests -j1` 已通过（首次真实编译19.80s，冻结warm再次确认）；首条误重复--tests只触发cargo参数拒绝，不算编译执行。详见 docs/validation/G4.4.6.3-lower-stage.json。两个私有新测试已编未执行，未启动linked/native。

静审下游：LLVM pure表示、局部storage layout、Array length/index、snapshot plan/classify与LargeCopy v1完整typedABI均已在基线覆盖该普通纯值形状，没有必要LLVM修改。closure/capture资格是已有独立profile，本片不从纯物理布局授权它们。Rust四目标：完整语义Type及真实名义/retained元数据保留、同一余额预费、旧机器大值保护不绕过、Lower两文件闭合；突破是修复Array外壳漏门，不冒充source/native或父项Done。实际Source重放、预算及独审待root安排。
