# 166 作者验收：拥有错误结果

生产提交 `dbf99ce24742f4796cb182d751e6676280666d85`；旧测试契约迁移
`fc03263d5fd3dfad17e3c40a28e37197bfee73ff` 只改 `verify/src/package_tests.rs`。
提案先于代码提交 `1528cd1`；共同 Schema/Wire 19 及两个指令由 root 提供。
父线负责独立审核、组合及全 workspace。本收据不替代主线最终验收。

## 实际闭合的链路

每侧为有限 Copy 或完整 Package，至少一侧 Package 时联合本身为非 Copy 普通 Value。
MakeErrorUnion 消费拥有载荷，TakeErrorUnion 消费整个联合，DropErrorUnion 只清活动
分支。独立 Verify 从真正 CFG 和责任流检查准确 tag、一次转交、活动责任及清理，
不调用 Types/Analyze 求证。存放、普通函数参数/返回和连续 `?` 已走真实源码、LLVM
和原生程序；Copy 活动分支不会使拥有联合可复制，非活动字段不会获得清理责任。

22 份源码原生构建/执行全部经过一次独立 Verify，O0/O2 各 11 份。关键观测：

- 两层普通函数转交与跨三模块返回，成功 Package 只输出一次 A。
- 两侧均拥有的 Q!P，成功退出 ALM；错误经两层 `?` 传播后 LME。最后 E 由
  native wrapper 清理，成功类型仍为 i32。
- 未取出及表达式丢弃 AEEA；continue/break 每次实际责任合计 AB。
- 后续实参失败 AZ；真正调用消费者则 CAZ；暂存参数仅清理一次。
- 新绑定第二字段准备失败 BZAC：先清新值的已完成部分 B，再按顺序清 guard Z
  和既有联合的两个字段 A/C。
- ByteBuffer 成功载荷与返回的错误载荷另外执行 4 次 realloc/free 观察器。
  C 观察器要求恰好 1 次分配、1 次实际 free、退出时零活分配；输出 FA 与 MFA，
  证明 free 在 Package 后续字段 A 的 Drop 之前，且错误 wrapper 先经过 main 局部 M。

12 个源码拒绝覆盖重复移动、部分 Package、旧 Desc、旧字节窗口、歧义注入、错误类型
不匹配、直接资源模式、give 普通联合、CT、捕获、整体重赋值和非法入口成功类型。
9 个恶意候选分别走 raw 与 wire，共 18 次独立拒绝：重复 Make/Take、Load、Read、
反分支或无证据 tag、转交后读 tag、漏 Drop、拥有联合捕获。变异后不重新运行 Analyze。

## 回归及真实失败保留

最终定向回归为 driver 10 套件 114 通过；5 个生产 crate 的 lib 297 通过；
Analyze Package 预算 5 通过；Verify ByteBuffer/owned-bytes 38 通过。
合计 454 通过、0 失败；**明确过滤 1 条共享版本旧断言**，不计为通过：
`retained_tests::current_wire_accepts_current_version_and_rejects_other_versions_and_every_truncation`。
该旧断言把当前 Wire 固定为 18；root 指定 168 的 `a3bf919` 统一迁移，166 分支不重复改。
最终主线必须带该迁移重新运行。lib 与新 driver 测试限定 Clippy `-D warnings`、fmt 通过。

`old-contract-failures.log` 保留最初 lib 295 通过/2 失败。另一条是两个 ErrorUnion
仅重标 local 的旧夹具，实际仍由 E-VERIFY-TYPE 拒绝未经注入的 Store；fc03263 将它们
拆成精确 raw/wire 断言，未放宽其他容器反例。`first-analyze-unsupported.json` 保留第一次
跨层准入遗漏；`whole-reassignment-discovery.log` 保留整体重赋值近邻的真实顺序失败。
旧 CLI 的 Types Unsupported 和二进制/源码摘要见 `baseline.json`。

## 尚未开放

直接 Managed/Desc、地址、闭包和递归拥有联合载荷；拥有联合捕获；非 Copy callable
参数/结果；整体 owning-union 重赋值。最后一项需要保留槽语义：简单 Drop 后 Store
在较晚 guard 仍活时不能维持现有逆成功构造顺序，本片 Types 直接准确 Unsupported。
入口成功类型保持 void/i32，`main()->E!Package` 不在此片范围。没有预算默认值增加。

## 复跑

先 source 公共 toolchains/env.sh，使用独立 CARGO_TARGET_DIR。命令使用 --locked --offline。

```sh
cargo test -p zil-driver --test impl166_owned_results
cargo test -p zil-driver --test impl056 --test impl072_packages --test impl074_packages --test impl077_package_captures --test impl079_package_captures --test impl099_owned_bytes_native --test impl155_managed_replace --test impl161_nested_closures --test impl163_resource_methods
cargo test -p zil-types -p zil-analyze -p zil-verify -p zil-lower -p zil-backend-llvm --lib -- --skip retained_tests::current_wire_accepts_current_version_and_rejects_other_versions_and_every_truncation
cargo test -p zil-analyze --test package_budgets
cargo test -p zil-verify --test impl095_byte_buffers --test impl099_owned_bytes
```

完整阶段文件留在 run-logs/166；Git 证据只有紧凑清单、必要原始日志和基线源码。
本目录约 120 KB，不含二进制、LLVM 执行产物、target 或缓存；源码交付包应排除此目录。
