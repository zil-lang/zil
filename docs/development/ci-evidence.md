# CI 与结果保存

当前主线有默认工具链与研究 LLVM 两条独立验证工作流；各自按真实 checkout、工具链、测试范围和结果解释，不能用其中一条通过替代另一条。具体步骤以 `.github/workflows/verify-main.yml` 和 `research-llvm.yml` 为准。文档与布局另用轻量检查，不运行语言全量。

CI 汇总为精简收据，保留源码 SHA、工具链、各步结果和必要失败字段；`continue-on-error` 的上传失败仍应看原始 outcome。测试通过、附件上传成功和远端可恢复是三件独立事实。Actions 的日志和附件有保留期限，不能代替 Git 源码备份。

```sh
gh pr checks <PR>
gh run view <RUN_ID> --log > exact-run.log
gh run download <RUN_ID> --dir new-receipts
```

读回必须核对实际 run、checkout 与附件摘要。当前运行是否含 raw、native、冷恢复或产品测量按其 manifest 判断，不能从历史脚本存在推定本轮已执行。旧额度/账单失败不会被新运行通过改写；额度受限时仍执行本地适用门并明确远端未运行。

`tools/ci/evidence_transport.py` 可核验及恢复采用其格式的历史完整日志归档。只有某运行确实输出完整归档且远端日志可读回，才能主张已恢复；本地输出不是远端保存证明。缺块、截断、run 身份或摘要不符必须失败。恢复使用新目录，原始结果不覆盖，详细保活/打包约束见工具自身协议和[交付说明](delivery.md)。
