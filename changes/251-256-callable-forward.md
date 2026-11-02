# 同一回调实参原值返回

继续用户指定的下一语言主线。以已完整本地验收的拥有Event/Reply和路径修复候选aebc4a600为代码基线；PR18远端归档重跑独立进行，新阶段不得抢先合入main。此枝先冻结公共合同/表示，不把可编译公共接口或局部测试计为新语言实现。待PR18完整证据核回后顺序集成，失败只阻塞真正受影响工作。

目标是普通自由函数的唯一按值F→裸F，返回本次已准备实参的同一有效实例。覆盖Fn/FnMut/FnOnce、旧有限Copy与同步Address调用签名、本轮完整拥有Event/Reply，以及完整普通拥有伴参；通过真实跨模块原源、独立CFG来源、声明模式、实际责任和O0/O2逐分配点清理验收。具体Copy实参复制而非一律移动；符号F bound不授Copy，结果恢复调用者原声明来源，不能统一降格或按实际Closure类型升级。

公共IR新增CallableReturnSource::{Fresh,Parameter(u32)}与CallableCleanup::Returned。索引为实际函数参数位置；source是未核关系，不能用ClosureId/返回类型相同代替实例证明。原Fresh工厂资格不变。schema/Wire/typed指纹迁移31，返回合同指纹3，角色合同指纹4；旧Wire字节和失败证据保持并明确拒绝，具名新版本反例另存。新增字段与遍历、队列/状态增长均预付现有额度；Fresh返回合同多一来源tag，Parameter多tag与索引，格式长度变化如实记录，不提高默认限额或免除必要工作。

251 root持规范/公共IR和机械Fresh迁移；252前端、253 Analyze、254独立Verify、255 Lower/LLVM、256真实消费者按不相交写集并行。所有实现以同一冻结提交为输入，只root发布公共接口和集成。Verify不复用Analyze/Types求证；非作者审核及组合验收后方可合入。本批不包含一般F容器/多F、借入逃逸、opaque新签名工厂、CT、外部长期保留或完整ZUI承诺。
