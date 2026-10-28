# 205 Expand 表头施工检查点

规范身份为 `ce4a2f0e3310fe83d7ed59e8646789297860f26e865f61b757c6a6af4f49b056` / `8d6fd152597c8446f455bc5a73807f5d056ec6a93f1b7c961a6a176a62b66534`，schema27 保持。本检查点供 Types/CT 联调，不是205验收。

源分类把一个完整 State 和一个独立拥有 F 与旧双 F 构造区分。早期参数化检查对未实例化普通 T 同样核完整返回类型、Package/直接字段形状、有限 Copy 伴随值和新 callback 结果边界；后期已展开的 Package 按实际字段位置生成一个 State binding 和一个直接参数。参数顺序与唯一 F 类型槽相互独立。

`run-logs/205-expand/header-check01` 的限定 cargo check 通过。`header01` 因新测试误写 Core 字段 `params` 而编译失败，未执行测试；修为真实 `parameters` 后，`header02` 的4组源码 Expand 测试通过。两轮均为11b3基底加当时未提交补丁；原源码/输出及补丁按真实身份保留。不把 Expand 通过当作安全核或 native 通过。

`baseline01` 使用生产字节相等的冻结204 CLI运行原4份输入：原138字节表头和两个混合别名输入先在 Expand Unsupported；旧单 State+普通 T 的原02输入实际 Success，但其未调用模板已从 Core 省略，只 main 达 Verify1。原02没有 native 执行，不称已执行内存错误。root 已明确授权修复这项旧定义检查遗漏。该位置/别名/循环修复仍在后续施工，本检查点不宣称完成。

Rust四目标：本片只有源定义与公共表头表达增量的有限证据；安全、运行/内存、维护与编译性能的产品级比较均未建立，没有产品级净突破。默认限额及产品门未改。
