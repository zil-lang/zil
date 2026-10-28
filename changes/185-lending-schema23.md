# 185：真实环境 lending 关系的公共声明

依据 ZIL-LANG-1.4 §9.4/9.7；本片由集成方单写公共 schema，不宣称语言能力已贯通。

## 改变

ClosureDefinition 与 CallableReturnContract 各新增 `lending_captures: Vec<u32>`。字段指共享 Address 结果可能借自的实际环境捕获位置；按下标严格升序去重，范围小于实际 captures 数，长度不超过 max_locals。它不重用 hold 或 writes 的既有含义，不把显式参数来源混为环境来源。普通函数参数来源合同不变；184 首片 callback 显式参数仍有限 Copy。

独立 Verify 必须从可达 CFG 重建真实结果来源并核对精确集合。非 Address 结果必须空；首片共享 Address 结果必须有非空真实存储来源。FnOnce 及消费适配不能返回其环境内裸地址。Address/Desc 捕获不能冒充自有存储。来源列表不能代替 Hold、活动 loan、清理和失效检查。可借存储的具体类型与机器路径由184独立交付；185占位一律拒绝非空新能力，不能通过字段制造权限。

## 生产者和消费者

Types 声明、Analyze 来源、Verify 独立重建、Lower/LLVM 真实环境、IR Wire、模块及 callable 返回合同指纹、导出接口 JSON、跨模块 remap 与原始夹具全部迁移。捕获下标是环境内部位置，不能按 closure/function 全局ID重映射。

schema/wire 23，Typed 指纹域 V23，callable return 指纹域2。旧版本0–22及未知未来版本拒绝；历史证据保留原身份。列表长度在复制或分配前按现有确定预算收费，默认额度不变。

## 最近反例与验收

非空声明未实现前必须 Unsupported；恶意超长向量必须 ResourceLimit。编码/解码往返保留两个字段，修改任一字段改变对应指纹，导出JSON不能漏项。184需补真实源码工厂/共享调用、FnMut结果窗口、环境销毁/移动/相交修改及临时消费路径反例；该后续生产实证不可由本声明代替。
