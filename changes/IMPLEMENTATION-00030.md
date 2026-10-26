# 030 可写闭包环境

按 ZIL-CAPTURE 的既有 FnMut 规则，实现 move 闭包中纯 Copy 值/聚合状态的真实原地更新。闭包是否拥有捕获与调用能力分别记录；写拥有字段得到 FnMut，调用要求真实可写且已初始化的位置。独立值副本有独立状态，let 位置不能通过隐式复制或改走 FnOnce 绕过。

IR 新增 WriteCapture 与 CallClosureMut；FnMut 隐藏环境 ABI 为 ptr，读取/写入真实环境字段。普通 CallClosure 仍按值传只读 Fn，独立 verifier 拒绝把 FnMut 放进该入口。只允许写纯 Copy 字段，地址字段写入及默认独占捕获在完整授权流贯通前保持 Unsupported；不会制造隐藏 Box/RC。

聚合捕获使用有限的私有约束图，补齐记录、元组、数组和投影的类型关系。仍由普通 types/analyze/verify 检查产物；图节点不成为公开类型占位或执行权限。schema/raw 版本由 root 集成统一更新。

普通 Copy 局部投影的复合赋值按 ZIL 既定顺序展开：左到右将动态索引求值一次并放入卫生临时，读取原值，然后求 RHS 和提交写回。只接受局部根；不是对任意返回临时或别名位置的授权。

迁移旧验收期望：move 写捕获在 let 调用点报告可写位置错误；FnMut 不能伪装普通按值调用。analyze 已有纯 Copy 聚合形状预检的费用计入原 exact-work 回归（每根类型两个访问），深嵌套 Tuple 改检验选定深度界限。
