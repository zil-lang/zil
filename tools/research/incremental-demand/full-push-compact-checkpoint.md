# 原生直接首改当前结果

正式源码 `861e9bb692baca50f9c7b2e8bbeceb5e7dfb8727`，window25原30工作流/10功能格、另12运行包装构建/4运行格完整通过。原5105函数/257输入、O2/CGU16/compilerjobs1；bounded-classes-128-v4和host-jemalloc显式研究默认关。

重启直接首改 **1.477s / Rust1.100s，仍慢约34%**；冷编译 **2.947s / Rust6.687s**。启动单列，不冒充普通CLI或OS冷缓存。用户缓存边界峰约43.1MB/R61.5–76.7MB，SDK/宿主/evidence除外。

冷保存到重启恢复整链改为完整等价计划物理共享：248份计划检查/4857共享，当前只重算1函数、恢复5104；Analyze正式35–43ms。原五项费用、当前完整绑定、独立Verify/预算/O2保持。strict/Analyze316/Driver251（含ignored原生）/fmt/release全过；未变IR94/backend382/原生发布3/qualifier31及独立核收据复用。

配对阶段诊断使用同一cold→退出→直接首改，无无改预热。Rust原O2/CGU16/jobs1额外开启time-passes/incremental-info；ZIL显式profile-compilation。仅诊断，不作为正式比值。ZIL命中14/16对象却仍机器重建/变换约406ms；Rustcodegen_crate78ms，其中LLVMIR约6ms、CGU复用判断约1ms。RustLLVMthinLTO443ms与finish361ms异步重叠，不能相加。ZIL对象阶段109ms；Source物化、函数缓存读取和语义发布仍显著。

Rust实际参考点是[先判断CGU复用再构造LLVM模块](https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_codegen_ssa/src/base.rs#L1238)；ZIL当前rawcache停在class归类前，进程重启仍重建完整当前图。后续需把恢复消费覆盖到归类/分区前置链，保持当前输入/原费用/完整机器检查，不能仅增加对象命中或持久授权。该后续尚未实施。此前缺少Rust同路径阶段分账是优化选点的明确缺口，本次已补齐。

诊断工具两次失败及更正、完整分账与原始位置见[当前JSON](current.json)。实际main默认ResourceLimit、工业大工程/普通CLI/全面Rust目标仍开放，未合main。旧结果严格绑定原源码，见[window24固定负结果](https://github.com/zil-lang/zil/blob/033030437ee0b29c81b7442d2cb2e6f7cd9b4279/tools/research/incremental-demand/native-direct-edit-chain-20261116/current.json)。

唯一施工分支research/full-push-native；goal/archive不合main。SDK18/native-link复用，不建LLVM、不全量打包。
