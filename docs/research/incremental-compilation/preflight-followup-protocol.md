# 当前机器预检后续窗口（测量前冻结）

起点：801933a9e及第五阶段已冻结五窗。大模块对象命中后的机器预检仍有明显成本；先按 `changes/research-preflight-fanin.md` 做独立等价控制。生产候选另以实际提交与ELF SHA绑定，不能把新结果归到801或改写原五窗。

固定9族：large-128、large-512、large-2048、large-4096、large-8192、even-64x64、large-skew-8、body-16x16、types-32x16。保留大小梯度、小输入成本、均匀/偏斜分布、复杂函数体和512名义类型实例。沿用283c661b生成器、完整4字节结果、cold→unchanged→early_edit→late_edit→restore，6个AB/BA交错区组、两CPU、8CGU/jobs2、默认预算、独占性能窗口。不得按测得比值删样本。

1. followup-default：9族×O0/O2×5请求×6区组×2臂＝1080请求，当前候选对Rust。
2. followup-closed：9族×O2×5请求×6区组×2臂＝540请求，显式封闭候选对Rust。
3. followup-isolation：9族×O0/O2×5请求×6区组×2臂＝1080请求，801默认对新候选默认。

共2700请求、225单元，单元比值与配对bootstrap沿用原协议。对象报告的分项仅解释机制；多CGU分项相加不等于并行端到端时间。当前预检在命中/未命中时均必须执行，不以缓存权限替代。

原23族全形状再做O0/O2各一个cold区组（92请求），14真实对照做O0/O2各一个cold/unchanged区组（112请求，另14参考构建），作为最终源码组合控制。其计时不与正式六区组结果混用；两个原默认超限族仍保留，无效速度比仍为空。此后续不代表大企业仓库、Cargo多crate、随机语料或Windows。
