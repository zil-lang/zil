# 177 公共接口独审的冻结夹具

`review177_public.rs` 是独立审核者 `/root/store_field174` 在精确提交
`061e22536784f4e568dfbcebbb8ca2086cdc54a7` 运行通过的完整 7 组源码。
该提交只在公共实现 `aa235a3379492221197b4686bec042c042c4e100` 上添加审核
测试，没有 174/175/176 的语义实现，也没有生产代码修改。

完整 7 组中的以下两组检查当时明确的语义占位拒绝，只对该精确公共版本有效：

- `raw_and_wire_transport_never_bypass_declared_semantic_stubs_or_local_preflight`
- `package_address_predicate_does_not_widen_copy_pointees_or_verify_authority`

后续 175/176 会实现这些公开指令和类型，因此不得把旧版本的 Unsupported
措辞伪装成最终版本的永久合同。本目录不属于 Cargo 自动测试发现路径；
其余 5 组仍保存在 `crates/zil-driver/tests/review177_public.rs`，用于最终
集成时继续检查实际传输、指纹、预算与名称边界。

要复跑当时完整 7 组，必须 checkout 上述精确审核提交（它已包含被审核的
`aa235a3` 公共实现），并使用全新独立 target：

```bash
git worktree add /tmp/zil-review177-public 061e22536784f4e568dfbcebbb8ca2086cdc54a7
cd /tmp/zil-review177-public
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
CARGO_TARGET_DIR=/tmp/zil-review177-public-target \
cargo test -p zil-driver --test review177_public --offline -j2 -- --test-threads=2
```

原始命令、首次夹具失败和修正后的真实通过收据在
`evidence/reviews/177/public/`。这份冻结文件只保留原始审核实验，不能替代
当前分支上的生产语义验证。
