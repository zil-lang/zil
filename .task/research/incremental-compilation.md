# 增量编译纵向专项

状态：Doing；长期研究分支：`research/incremental-compilation`。

用户于本会话授权建立新的专项、持续攻坚编译速度并直接使用 GitHub 权限发布。正式父目标为 B0/W4；本记录不变更父工单状态或产品门。

- 基线：`21058d0e331de764e3a1b9123c3e1edf5e55a1aa`（开工已回读 GitHub main）。
- spec_hash：`b94813aee9a57ce9810a5fe59e3ed7a060c3fead985c5cd6632c9deadf4abdcb`。
- normative_hash：`5e0b190fff701bcaef8ffb1cbd818261584ee2795f69dad514668e497719f3c1`。
- 当前权威记录：本分支本文件；复用已有隔离 checkout，不增加并行写入者。
- 首片写集：`crates/zil-backend-llvm/src/machine_cgu.rs`、`crates/zil-driver/tests/object_cache212.rs`、本记录、`docs/research/incremental-compilation/**`、`tools/research/incremental-compilation/**`、`evidence/research/incremental-compilation/**`。
- 公共 schema、Cargo.lock、默认逻辑预算、产品门和语言接受性不在首片写集。
- 首片产物：精确声明依赖的 CGU 分区、真实源码对象缓存反例及必要反向控制、限定测试、独立审核与精确提交收据。
- 限定命令：`cargo test --offline --locked -p zil-backend-llvm --features llvm-api --lib`；`cargo test --offline --locked -p zil-driver --features llvm-api --test object_cache212 --test impl083_bytes_native --test impl120_field_backing_native`。
- 阶段状态：施工。形成可编译检查点即提交推送；独立审核绑定实现提交，未经审核不宣称 main 集成或性能突破。

完整路线、旧图切割/zit 的适用边界及四目标见 `docs/research/incremental-compilation/PLAN.md`。后续片开始前在此明确追加写集与控制，不把待做路线记为已实现。

## 往来

- 2026-10-31：从远端 main 建立专项；首片优先消除未用外部声明对所有 CGU 的失效放大。当前 Session 容量断崖、整项目语义重做及核验/Lower 重复成本已定位，后续依纵向路线继续。
- 2026-10-31：首片实现 `665a0a06dc36a94f19e9ee8bbcd87fab321c6423` 已推送；限定64测试通过，后端Clippy通过。按独审意见收紧错误拒绝断言，最终功能审核绑定 `8fb69d61b41a241806e7021de93a27ce7e213f7b` 为Pass。200次完整CLI screen和200次native均通过；收益限于声明失效控制，保留private编辑约1.1%不利点估计，未宣称产品突破。长期专项继续Doing，main未集成。
