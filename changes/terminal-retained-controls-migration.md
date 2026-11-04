# 当前能力组合下的原有控制迁移

原始 full default 源8981bbccb结果4170通过/115失败、49个失败目标，保留失败记录。impl187发现的真实符号捕获Apart遗漏已经修复，原源码及raw/Wire反例不删；所有无效源继续拒绝。

普通用户enum改名LegacyOption避免新内置名称冲突，生成f32/f64同族helper改为独立前缀；独立手写LIR布局补F32/F64实际4/4和8/8。源NumericConvert负例直接篡改当前节点，保留Bool非法转换。冻结Wire34字节不改写，测试专用显式版本迁移后重新进入独立核，旧字节版本拒绝保留；255原本就是Unsupported传输，未知opcode用253/254。

Option别名、无捕获借入薄函数指针、关联类型声明、完整拥有Tuple和合法Copy字段借用返回按已经实现的消费者提升为实际正控；原逃逸、整owner复制、父失效和不完整转移负例保留。资源头/Store字段raw攻击连同实际ReborrowAddress类型一致修改，恢复目标权限/owner检查，不能用任意早期类型错误替代。

泛型定义侧State别名检查更早拒绝的源，明确记录Expand/E-GENERIC-ARGUMENT及具体原因，不宣称到达Verify；现有独立raw/Wire攻击继续覆盖安全核。拥有字段替换仍因实际未清理责任拒绝，普通资源头Copy字段写的缺口交342，整owner替换交340，不把拒绝类别迁移当成功实现。

旧预算常数需对当前完整源重新测量N/N−1/fresh，保持默认Limits。原记录数字及源码在Git历史保留，迁移收据须绑定最终生产源；测量辅助仍保留显式精确数字断言，不能把动态测得任意值自行标为预算验收。终态全量与产品条件未通过。

The integrated associated-constant declaration walk adds real paid work on old
sources. On bfe682bfb, measured Expand control minima become nominal inline
1008/265 and where1111/309; plain864/330, alias1845/549 and combined2021/610;
copy helpers811/447 and1090/646 (Expand/Types); legacy State2766 and ordinary
Store3360. The preserved generic error controls are1275/496,1205/456,1812/638;
direct enum977/412; concrete resource preparation509/279,976/530,973/531;
generic Enum468/201. Defaults and sources stay fixed, with N−1 and fresh/warm
checks retained. These limited repairs do not erase the actual aggregate stack
abort in the same run, which remains a production blocker pending repair.
