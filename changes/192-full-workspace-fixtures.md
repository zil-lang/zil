# 192 全仓旧断言迁移

精确主线 ea82cf71f63608bee392b6888cb00616e091bccf 在独立初始为空的 TMPDIR 运行全仓，1925 Pass / 3 Fail / 0 ignored，202 test groups；原命令和全部日志在 run-logs/192-integration/formal-workspace01。三个失败均为既有输入在192实际语义下的新结果；本变更只迁移测试，不改生产、输入、默认额度或Verify门。

- impl120 保留原 resource R{x:i32}、Drop、Q 和 b 窗口原文。192 已独立核有限实际名义 payload，因此 r 替换与 b 后备不同；原整包冲突断言改为 Success/一次 Verify。其余同字段、分支多来源、整体移动等反例不动。未验证/非Copy payload的拒绝继续由192独立raw负例覆盖。
- review176 的 field-is-copy-data 与 review179 的 review-copy-index-is-not-callback 保留相同篡改：实际字段改为Copy而结果还谎称callback地址。192允许对Copy字段借址后，独立Verify在实际结果类型不符处精确拒绝 E-VERIFY-TYPE；raw与Wire仍必须SafeReject，不能改成允许或宽泛错误类别。

原全仓失败保留，修正后运行三套限定测试，再在已集成候选上做唯一TMPDIR正式全量。不用测试数量宣称完整1.4、产品门或性能收益。
