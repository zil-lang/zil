# 064：有限非逃逸具体环境 helper

## 先行提案（2026-10-24）

按设计第9章保留真实环境身份、调用模式、Hold、Call和Cleanup，扩展一个静态具体环境参数的普通helper。源码使用既定`F: Fn(...) -> R`、`FnMut`、`FnOnce`及`F`/`&F`/`&mut F`参数。先保留模板，在实参环境身份已知后产生独立具体函数；不能把环境擦成薄函数指针或在caller内联私有正文代替公共合同。

CoreFunction保存未核CoreCallableParameter；TypedFunction保存未核CallableParameterContract。具体参数沿用Closure(id)与Address<Closure(id)>。新增CallClosureBorrowed以地址SSA和显式Fn/FnMut模式调用实际环境；不能LoadAddress复制非Copy环境。按值参数使用既有责任转交、准备临时、调用与销毁。

合同记录参数序号、具体环境、请求调用模式、捕获地址Hold、调用目标依赖、写效果上界、Owned/Borrowed清理类别及所有有限Return/Trap调用轨迹。Return包含普通值以及Copy E!T的任一标签；本片不声称按错误标签区分接口关系，实际成功/错误流程另由CFG和native验证。Hold不能把Desc误当持续地址窗口。候选声明不产生权限，独立Verify从定义重建后，caller仅消费已核关系，driver只导出Verified合同并纳入接口指纹。

本片每路径最多两次callback调用，无helper递归、循环及helper转发；callback显式参数和结果有限Copy，支持已有Copy错误联合。Fn/FnMut定义的受核操作保持外部资源责任；未来涉及Desc撤销的新操作必须扩展效果域，不能默认空。Fn到FnMut/FnOnce以及FnMut到FnOnce的适配必须有实际ABI与剩余清理实现并独立验证，不能只改标签。

最近反例包含借入环境被销毁/移动、借入后丢失原captured-address Hold、伪造调用次数/模式、删依赖或写效果、未知合同、循环/递归、未调用的拥有环境漏清、后续实参失败漏清和第一次callback错误后仍第二次执行。作者限定测试、非作者精确候选审核、root正式全门；不改变产品门及原性能预算。

schema/wire升至10，Typed指纹域同时升级。新增Core/Typed/interface字段所有生产者与消费者同批迁移；旧raw输入不按新协议静默解释。Lower/LLVM保留真实环境和固定捕获清理顺序，借入环境不产生拥有债务。

## 算法依据与范围

具体实例缓存按模板身份、具体环境身份及已核类型参数键控，采用显式待办队列和预算，不递归展开无限实例。参考Rust canonical goal/cache将可重用问题与调用点临时推理变量分开，以及Zig按实际类型需求分析的原则；本片不是完整trait solver，不宣称复现其通用求解能力。真实接口和受支持能力先实现，未使用函数模板仍需符号/绑定合法性检查。

一手参考：Rust Compiler Development Guide `solve/trait-solving.html`、`solve/canonicalization.html`；Rust 2026 next-solver project goal；Zig 0.16.0 release notes的Lazy Field Analysis、Reworked Type Resolution与Incremental Compilation。2026目标中的globally稳定化不是已发布完成事实；Zig增量与LLVM Emit Object是不同成本层。研究063另给原文链接、实测和适用性分析。

## 063成本定位带来的driver修复（实施前追加）

生产driver在未请求阶段产物时也提前构造完整Debug字符串。采用063冻结对照的lazy-dump补丁：仅在ArtifactPlan存在目标时才计算文本，之后仍沿用同一取消检查、路径身份检查、原子写入与发布。输入/缓存/Typed快照的必要digest计算不变；请求emit时文本逐字核对。这项是独立的driver成本修复，不记为图切割独有收益或省略Verify。
