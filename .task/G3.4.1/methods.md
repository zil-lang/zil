# G3.4.1 普通泛型 inherent 方法实施记录

Goal G3.4.1；父合同 `_zil_design.md` §18.1/V10 与原 `tests/language-readiness/terminal/342/slice_iter.zil`。实施者 general_priority_review；基线 main c63001c8e，生产交付 `a4f25e1e75d3a86a25452b9010092672b1734e48`。独审与合入由root裁决，本记录不是实时goal看板或完整lending验收。

规范 spec_hash `e235b19010325a125338f764f9e75b8ab6dd612f75ea4915e4761edd4a7e0b7a`；normative_hash `e938daf9f797c6790549c578e25d474d6eafeba20bf50ee51b282fbaa653f80b`。依赖life词法提交 e48ca128（本枝cherry-pick 5a0a12e8f）。

写集为generic_inherent及既有Budget/Engine/source class/存储header/调用入口的必要接点；不改公共schema、锁或正式性能门。方法保留真实声明/member来源，完整impl life/type向量与Self target；普通类型实例和实际life合同分别核验。含life的具体方法在自身scope发布stored header再进入参数推断；life-only方法沿普通retained函数。借入affine nominal不要求载体整体Copy，字段访问/真正Copy义务/alias/source检查保持。

最初新Cargo测试目标已撤除；当前只挂既有 `zil-expand --test generics` 的test-support。组合源码 `8894af608a6693a47d27f9f95fc6ee8ea3ad1905` 已真实22/0/0/0；核心方法文件与交付逐字相同，shared producer依赖按组合运行记账，映射/命令/日志哈希见 [收据](../../docs/validation/G3.4.1-methods.json)。其中8项新测试覆盖普通共享/可变receiver、两种具体T、未使用定义负控、同名字段/方法、错特化/重复方法、stored Cursor、两life与impl binder重排/错绑定、life-only、跨模块pub/private、private nominal经pub factory返回后的pub method拒绝及预算。

新普通方法源的最终边界冻结work1431/nodes484；N/N−1/fresh真实执行，全部旧控制原N保持，默认额度不升。早先1418/483为加入必要字段冲突/lookup核验前的历史新输入费用，不是最终阈值。全target严格Clippy no-link已过。

历史Fail保留：Copy holder整体误要求、callee结果错误使用caller scope，以及combined db1e0f7的private跨模块21/22。最终private修复在既有Impl预费遍历提早触发真实source identity ledger，恢复flatten前原module归属；不解析生成名、不用Span猜权限。原失败日志哈希与修复/成功运行映射在收据中。

此片只证明有限Source→Core方法组合。原SliceIter字节不改，完整Option返回/两核/raw/Wire/原生与失效由G3.4.1合流验收。Const/where/方法自身泛型/特化/隐式receiver继续Unsupported。Rust四目标：有可重放semantic-feature增量，实际安全仍由后续独立核消费；同语义运行/RSS/p95、编译和维护成本未测，产品净突破及四目标合取不升级。

最终补核：root source reviewer指出private nominal的pub method不能因pub factory返回而泄漏，Method跨模块资格与lifted helper导出共同要求host与member公开；same-module保持。实现a4f与根8894组合运行期间源码未变，完整existing generics再次22/0，N1431/484固定断言已真实执行。前一b083的22/0与更早db1e的21/1分别保留，不叠加测试数量。
