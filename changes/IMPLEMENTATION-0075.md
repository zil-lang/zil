# 075 当前 raw wire 版本回归夹具迁移

## 原因与依据

精确集成提交 `04f7f1001056258e844b3bc8929847e2680496dc` 的正式门在
`retained_tests::schema11_wire_rejects_older_versions_and_every_truncation`
失败。该历史夹具把当前协议 12 放入非法版本列表，同时漏掉 0 和旧版本 11。
`spec/interfaces.json` 固定当前 compiler schema/raw wire 为 12，生产编码器和
解码器已使用此版本；本片仅校正测试预期，不改变生产解码器、语言规则或验收门。

## 最小变更

- 测试名不再声明历史 schema11。
- 对真实编码结果增加独立字面 header=12 断言，来源为已冻结规范中的协议版本，
  避免只用编码器/解码器共同常量互相证明。
- 完整当前字节实际通过 `verify_module_bytes`，作为版本接受的正控制。
- 非法列表使用 `0..WIRE_VERSION` 与 `WIRE_VERSION + 1`、`u32::MAX`；
  当前明确覆盖 0–11、13 和最大值，避免未来把当前版本误放进旧版列表。
- 保留所有严格前缀截断的拒绝检查。

写集仅本提案、`crates/zil-verify/src/retained_tests.rs` 和本片证据。
所有生产文件、门、默认预算、Cargo.lock、schema 和规范摘要保持既有内容。

## 验证与交接

作者从独立空 target 运行 `zil-verify` 的 `retained_tests` 限定集，记录真实命令、
退出码、完整输出、源码身份和结果。非作者绑定最终精确候选独立审核后，root 才
合入并重跑正式门。原正式失败保留在
`out/runs/draft15-audit/formal-failed-001`，本片不改写其结果。

本片不增加语言能力，不构成运行或编译性能突破；其效果是让固定协议边界具有
明确正控制与完整旧版本负控制。完整语言和产品状态沿用原门结果。
