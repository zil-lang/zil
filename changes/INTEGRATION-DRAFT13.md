# draft13 集成记录

2026-10-24 UTC。接续70d3a729的draft12主线，用户已明确授权draft12七件及后续成果保存；本轮不请求重复批准，不改产品门。

## 已纳入的精确候选

| 切片 | 作者候选 | 主线集成 | 非作者审核 |
|---|---|---|---|
| 063 编译成本/关系研究 | 5ce89c0eaeccc50a7d38ef4db5c23f90b1106d20 | 7efa64d9f03a840243e3ec4c61acf98bf6f3af10 | /root/helper_frontend |
| 064 公共合同/机器/driver | 602009d340b0593aeec8cf5a102af15a05568a4f | 5f45aa0e8acc836f20b0ede1513c182bac83ee3e | /root/helper_verify |
| 065 源码/Analyze | 65588a73a55c8f697a2f842221903a5471428313 | f837ab9c168818cca00c81fb71f2398fc500a97f | /root/compiler_research |
| 066 独立Verify | aa82ad5c108b3dabe652ebc115ab30c4635f8e71 | 随064保留原始候选祖先 | /root |

逐项review.json和作者收据纳入Git。063及066保留自身候选中的历史规范摘要；064及最终065包含公开schema10快照4aaffc56。当前正式任务与仍退回018更新到该当前规范，旧已关闭切片不追改。

## 接口和范围

schema/wire10、Typed fingerprint V10、opcode53以及结构化callable合同按0064提案同批发布。一个有限具体环境helper，callback为有限Copy参数/结果，最多两次调用、FnOnce至多一次；额外借入参数、循环/递归/转发、非Copy结果和环境保存/返回不接纳。公共helper调用方完全不读取旧callee私有效果行；定义仍独立Verify。真实环境ABI、晚实参失败、按值/借入释放责任均由作者原生对照和非作者近邻覆盖。

065独审找到跨模块同名helper内部符号冲突。ecfee852修正为定义模块/节点/环境身份的内部名字，保持模块局部导出；真实组合4976f8e从新target构建，实际native退出53。未将旧机器依赖缺口或Cargo缓存问题算成新前端语义缺陷，失败原件完整保留。

未请求dump的惰性格式化已按提案吸收，保留请求字节、必要摘要、Verify、取消和原子发布。研究冻结仍是schema9的70d+1c组合，完整计时和独立oracle不改写为当前schema10结果；生产关系核替换仍NoGo。集成后稀疏位置表示的静态合同补充在SPARSE-CONTRACT，不标成已实现算法。

## 验收和交付界线

本记录在主树正式门之前提交，正式门的真实结果只以随后生成的evidence/final-summary.json为准。旧target移到主树外保留，正式门从新target开始；所有原始运行、首失败与研究portable归档有明确范围。后续只允许证据/文档/交接元数据进入打包HEAD；生产源码修改必须重新取得对应正式证据。

完整语言NotSatisfied、产品门NotRun、B0 Doing保持。源码ZIP、完整main祖先bundle、audit及包外冷恢复分别绑定真实来源；用户授权的保存只有逐件成功回执后才报告完成。

## 首轮正式门发现的独立测试夹具迁移遗漏

ec62a86b 的正式运行 `20261024T004901Z-e3a8fa4e` 为 31/32，原始失败与已发布收据完整保留。`PLAN-T05-003/004` 在执行断言前，独立编译的 `tests/verify/boundary_harness.rs` 因 `TypedFunction` 初始化缺少 schema10 的 `callable_contract` 字段而失败；该标量函数不含 callable 参数，修正仅显式填写 `None`。未修改生产接受性、Verify 断言、协议门或预算。修正需经非作者审核、两个实际边界入口重跑，再从干净提交重新运行正式门；旧失败不被改写。
