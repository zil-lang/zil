7b08 精确冻结独立审阅

结论：本次对非本人 authored 的 root class Memo、Peirce 单次 wire decode/encoding capture 及相关接线，未发现阻止统一验收的安全或事务 blocker。未运行编译、测试、fmt、strict、release 或计时；不作性能收益结论。

已只读确认 commit `7b08cc846edbcd73b5e7f2ddb08dd3594c764fd9`，tree `da85c53e7f11e98e8be5beb71127002b357f3c5b`。审阅通过 git show/diff 指向该精确对象，主要比较基线 62d。本人编写的 checked-add Cache 不作为此次“独立审阅通过”的对象；只核对 root 的消费者接线。

**Class Memo。** 当前 triple 与全部 Function header/defined 上下文相等后，完整 Function Eq 才复用 dependencies/cost；纯输入决定的地址摘要即使跨上下文复用，也要求完整该 Function 相等。global initializer 地址与 protected 集合每次重新并入，当前地址/角色变化仍控制 DAG 准入和父节点能否进入 ready。normalized key 另核对真实 callee index→当前稳定 class 名，callee 类变化不会被 caller 原 body 相等掩盖。

selector 仅选择 bucket，bucket 内仍完整比较 normalized Function；不相等的同名 class 碰撞使暂存优化失败，不提交混合 module。回归源码覆盖代表成员变更、真正 callee 变化、protected 角色变化及人为 selector collision。Cache 仅持有当前 functions/reads/normalized 值，无前驱 Cache 或 Session 回指；Arc<Cache> 附既有 TransformProduct 四槽。build 在实际调用前仍完整 validate 当前原模型、按原 TransformPolicy 选择先前产品，并保留取消与失败回退。

**单次 wire decode 与 encoding capture。** `decode_module_candidate_with_encodings` 仍使用原 Reader 的 header、module、plan 与 finish；新增 recording 只记录该 Reader 实际 position 和 nodes 差值，不改变原配额检查或解码字段。`CandidateDecodingRanges::bind` 要求精确 segment 数量，并对全部 header/function/module suffix/plan header/plan rows/plan suffix 的实际消费范围逐段比较完整 bytes。caller 提供的 fingerprint 不充当 receipt，返回编码的 fingerprint 重新计算；plan encoding 绑定本次 decoded immutable owner。

边界或分段不匹配时，完整 raw decode 结果仍可返回，但 encodings=None；不产生第二次 candidate/function/probe decode。原 Reader 不支持的 wire 版本仍按原规则拒绝；driver 不支持的 memory layout/version 不获得快捷编码槽。正常未知 segmentation 不能迫使按错误 offsets patch。

**Driver 边界与费用。** 原 assemble 的 logical 总长预检仍先于 item I/O；MemoryPublication::restore 再核 declared/actual segment 总量。其一次 decode 同时返回 raw 值和可选 encodings，canonical raw owner 在 rebind 前保存，原 rebind 仍核 FunctionId/相对 span/checked offset。memory 快路保留完整当前 limits/header/function/plan 比较和新 metadata 编码。变化函数使用已封存 slot 的实际 node_cost，要求 limits 与 segment Arc 绑定匹配；新 node cost 增长仍回原 ordered writer，不能由旧 total receipt 放宽当前 aggregate quota。

LoadedSemantic 三元组、adopt_loaded 只导入 memory/pack hints、成功后才 install 的事务边界未被取消。load/rebind 后、save 前和实际 publish 前的取消检查仍在，packed manifest 成功发布后才安装 matching raw owners。没有增加 Verify 能力、终端 query 权限或持久 Source→Machine 授权。

已读新增 IR 回归源码：decoded 值与原值相等、shared bytes/plan owner 绑定、node cost 与 Writer 一致、limits 改变时重编码、精确 node/byte 上界与 N−1 拒绝、伪造/移动 segment 边界；driver 回归另覆盖 shifted boundaries 只禁用 slots、取消和加载前长度拒绝。以上是源码中的断言，不是本审阅执行的结果。

**相关链路闭合。** PartitionInput 已在完整 equality 与 same_recipe_environment 中绑定 class_symbols。PruneProduct 按每个实际 unpruned unit 的 class-declaration 索引集合复用，并同时比较完整 triple/items；不需要因其他 unit 的全局 class 集变化而拒绝本 unit，原普通声明保留策略的上下文也已补齐。checked-add 正常/回退入口的 checked/reused 计数累加，原完整当前 preflight 保留；Types 的 life.none key gate 与所有 life 在 get_inner 计费前 miss 均保持。

范围限制：Memo 的 reused/rebuilt 字段描述成功返回的 Cache；stable class 中途拒绝后，不能只凭这些字段排除已发生的试探工作，相关耗时仍在原请求内。62d 的三次恢复解码已被本次源码移除，但 bytes 比较、完整当前 preflight、恢复分配及剩余全局规划仍有实际成本，净收益留给 root 的同版统一结果。此前 62d Types 108/3 首败记录不被此审阅或新增断言覆盖。
