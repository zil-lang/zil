# available_externally 导入：历史与机制核查

结论：不实施、不采样。ordinary Complete 的 Internal 实现副本不能直接改回 available_externally（AE）。本项目已有完整 body 与 linkage 交叉的四臂消融：完整 AE 未完整恢复 branch 运行质量，诊断 optimizer 成本更高；完整 Internal 副本恢复。此次核查没有生成新性能证据，也不把旧诊断计时当作本轮正式性能结果。

## 精确来源

| 来源 | 精确 Git 身份与文件 |
|---|---|
| 首版 AE 导入 | `75531d43b12c6395797f2b4a6322dffa91db9506`，`crates/zil-backend-llvm/src/machine_cgu.rs` |
| 完整 Internal 实现副本 | `b9e566e02cf69b217a2d47c2377340771b02f161`，`crates/zil-backend-llvm/src/machine_cgu.rs`、`machine_cgu_clone_controls.rs`及`changes/research-incremental-o2-quality-closure.md` |
| 固定历史归档 | `8255079de3434de5aad44805a12eed8e9f433c24`，`changes/research-incremental-o2-quality-closure.md` |
| 首版机制说明 | 同一归档的`changes/research-o2-graph-partition.md` |
| 四臂工具与适用边界 | 同一归档的`tools/research/incremental-compilation/quality_closure/README.md`、`prepare_ablation.py`、`run_ablation.py` |
| 当时正式机制说明 | `b9e566e02cf69b217a2d47c2377340771b02f161:docs/research/incremental-compilation/QUALITY-CLOSURE.md` |
| 当前只读生产基点 | `99035931d358add0eaed6d4535035c38413701dd` |

上述归档中的四臂报告注明使用原实际 LLVMRunPasses 输入、目标、PIC模式、codegen等级、PTO设置、late MergeFunctions及对象链接顺序，并要求 bounded baseline replay 的 executable 与冻结物逐字节相同。只改变 root object，其他对象复用是显式的；原五种 native 输入 oracle 保持。报告记录60个消融 native 输出及20个capture输出匹配，明确属于功能诊断，不是正式性能比较。

## 历史反例的范围

旧 bounded-AE 策略仅导入机器成本不超过64的函数，每unit预算65536，并按定义ID顺序拓展，因此不保证root的完整调用闭包。冻结8192-arithmetic增量运行约慢326倍，但当时不能把该失败单独归于AE：闭包截断和linkage需要隔离。

实际root归因记录：8192-arithmetic root拥有1024个helper、导入2340个，O2后仍有4828个仅声明helper调用；2048-branch root拥有256、导入1129个，仍有663个仅声明调用。此后才做四臂：

| 控制 | 历史已记录结论 |
|---|---|
| bounded AE | replay与冻结坏质量产物逐字节相同 |
| complete AE | arithmetic运行质量恢复；branch运行质量未完整恢复；诊断optimizer成本显著高于complete Internal |
| bounded Internal | 大arithmetic运行退化仍存在，只有换linkage不足以补回缺失闭包 |
| complete Internal | arithmetic与branch质量恢复；额外512-body默认/closed捕获也恢复 |

因此，既不能说“AE必然对所有程序更慢”，也不能说“完整body可见便与私有副本等价”。历史已有同机制反例，不应没有新适用条件就再次把全部Internal副本换成AE。

## 当前模型和绑定已经支持AE

`zil-llvm-api/src/model.rs`的`Linkage::AvailableExternally`是仅供优化的body，`is_local()`为false；`native/builder.rs`映射LLVM C linkage为1。`preflight.rs`要求AE函数有正文并完整检查，拒绝AE globals。`tests/available_externally.rs`保留O0/O2真实控制：导入对象不发射第二个canonical symbol、最终链接至owner、函数地址相同；无正文、坏正文与AE global均拒绝。显式text-reference也有相应控制。

LLVM官方说明支持AE机制合法性，但不证明本项目运行质量或收益：

- <https://llvm.org/docs/LangRef.html#linkage-types>
- <https://llvm.org/docs/doxygen/FunctionImportUtils_8cpp_source.html>

## 私有副本的真实职责

当前`machine_cgu/layout.rs::Layout::replay`导入`zil.import.<owner>.<canonical>`的Internal实现副本。仅direct `Call.callee`改名；函数值、自地址比较、callback参数、global initializer和间接调用仍用canonical。完整direct-call闭包里的自递归、互递归也走对应真实私有实现；canonical owner仍唯一。

它不仅给inliner看body：即使未inline，unit内非inline IPO后的真实实现仍可调用并发射。历史选择理由明确写明non-inlining IPO；没有把某单一pass或参数专门化证明为唯一原因，不据此声称每个负载必然损失某项优化。

只将当前私有名的Internal改为AE不可行：没有其他owner定义该私有名，未inline残留调用可能无法链接。真正AE要使用canonical名字，去掉同unit重复prototype，并重新核对直调、递归、唯一owner及函数值地址。既有原生控制还包含`call @f(ptr @f)`、不同等体函数地址、C callback、共享global、直接/互递归，不可因切换linkage删除。

## 与本轮1679证据的关系

1679的既有机器码归因由其他代理单独读取，本核查未编译、未运行native：n64/base A根value约39577B、8385条静态指令、7个call并有大量SIMD；B根约911B、261条、63个call、无根SIMD，叶子自身SIMD仍存在，main循环和迭代次数相同。它支持“成本32丢失跨leaf实现可见性/inline与跨调用组合机会”，不证明完整AE可恢复全部原行为。

本轮1679的24次clean、216次native实际运行门已由root判为NoGo：八格B/A约1.616–2.123，全部越1.10，即使Rust/B仍大于2也不放宽。该事实属于既有runtime收据，不是本次历史审计新测量。

## 可省工作与剩余风险

AE不省完整导入body的lower、复制、hash、预检，也不省其实际依赖失效；改变被导入body仍会使caller对象miss。AE仍参与优化，历史complete AE有optimizer成本更高的反例；优化器还可能产生其他必须保留的内部实现，因此不能承诺所有重复emit都会消失。

全对象命中的暖请求本就不做LLVM优化/emit，AE不能解决这些请求已有的lower、规划、hash成本。对编辑miss，潜在收益只在部分未inline实现的最终机器码生成；不能据此宣称root和owner从两次miss变一次，也不等于函数级增量。

最终处置：本轮不实施、不采样、不新增生产变更。只有区别于历史complete-AE的新适用条件和实际优化结果证明，才有重新评估依据；当前未找到。
