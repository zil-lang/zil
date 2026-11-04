# 340 显式member-owner比较的实际剩余额度

c262 prepared-target方法在比较owner前调用独立type_snapshot_work完整规划，
再向caller收费；caller已接近耗尽时不能先使用完整初始额度遍历。该修正把
实际剩余slice交给规划器；每个已访问节点在成功/失败返回时均结算给caller，
不因规划失败返还已执行工作。额外比较/复制倍数同样先限制可用slice，owner
相等性只在全部费用已结算后比较。普通member_owner_work复用同因修正。

成功路径费用保持，N−1/fresh行为和两种旧复合位置求值控制保留。新增单位
近邻以长嵌套Record owner的深尾为哨兵：充足额度先见depth约束，caller仅余3
时必须先见work耗尽且剩余归零；合法owner按原计费N/N−1/fresh核验。实际
source-derived raw Core长owner及现有impl260目标由集成者在clean候选重放。

只修CPU/额度规划，不改地址/借用/拥有资格、ABI或runtime opcode。121–124与
ErrorArm语法消费者仍独立施工，本提交不将其登记或临时拒绝当作交付。
