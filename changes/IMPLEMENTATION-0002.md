# IMPLEMENTATION-0002：本轮实际实现与共享接口变更

用户授权将第一阶段工作交给集成者，并允许大量子代理并行实施。初始 B0 内按不相交 worktree 开发 base/IR/verify、syntax、lower/backend，集成者负责 types/analyze/driver 等正式生产链；随后按明确写集并行补前端归一化、查询驱动和独立核验。

## 接受的实施接口

- 在 Core 增加 `RecordUpdate`、`Sequence`、`CompileTime`，均是未核数据。Sequence 只固定求值身份/次序，不授予 Copy/Move/Borrow。
- SourceMap 作为旁车保存展开来源、角色和父链，带真实边界、UTF-8、唯一目标、父链与预算检查。
- raw CT 与 typed/candidate 字节编解码位于 zil-ir；decode 不创建已核值。verify_bytes 解码后仍调用独立 verifier。
- `VerifiedModule` / `VerifiedCTIR` 由 verify crate 私有创建；后端必须将公开 LIR 与同一已核模块重新绑定。
- CT 源展开只增加闭式标量能力，生成结果回到普通检查链；不改变资源和生成器规则。
- zil-expand 接受 T01 归一化写权，现有 T01 文档的 syntax 写集描述由本变更单补充；该转移不与 CT VM 或 types 目录重叠。
- 属性 parse 结果旁车新增 cfg 排除与待校验项；没有 cfg 执行器时，expand 明确拒绝未执行属性，禁止静默丢掉参数/字段属性。
- 产物发布统一由 driver 预先规划路径；native 输出先暂存，完成取消检查和 dump 后才发布，失败回滚旧程序与 sidecar。保留失败恢复目录用于发布回滚本身失败的诊断，不声称具备多文件原子切换或掉电恢复。
- analyze 在计算 raw 候选指纹前预检所有模块输入，所有函数共享累计工作预算；这不是资源来源证明的替代品。

## 完成状态处理

本轮获得 B0 运行证据，但共享 schema 全形状尚未完成，B0 保留 Doing。后续包已有可复用生产实现和限定测试，正式前置门继续保留。没有修改规范来迁就当前子集，没有将资源 Unsupported 或 parser-only 结果计入资源/闭包通过。

语言规范、目标产品门与依赖策略维持原冻结输入；实施细节和状态放本变更单、施工状态及 evidence。正式测试首次发现的工作树清理失败保留原始日志；独立复跑与最终验收另建 RunId。
