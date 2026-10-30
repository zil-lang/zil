# CP22 作者交付

源码实现提交 `be7b191e536a0b8fae009713c64f8d05ff3503d9`；定向验证及release构建输入提交 `096f1173a3a6f09bbf802d3daaded9e85495f773`。两者 crates tree 均为 `dd700c62ee21c8d2c955cc8106749dc0b84fd21e`。随后按根协调者明确授权，以 `f660aeb68e4cf86eab0bc7c9e219d5bcf8bdb5ea` 仅更新 CLI HELP 的 query-cache 描述；编译逻辑不变。最终 crates tree 为 `a598b9f6b04feff21fc2eed24566726a1a79ef12`，正式构建输入与测量应绑定该版本。

## 实现与边界

完整成功AST/token/attribute元数据的有界codec，parser+attribute真实预算收据，解析专用key与前模块剩余额度解耦；跨进程复用在现有显式 `--query-cache` trusted-local范围启用。源字节仍读、导入当前解析、按原顺序扣费；余量不足回原parser；成功请求后才建目录/原子保存；独立Verify照常执行。

磁盘语法复用与session复用、CP12整包Typed checkpoint分别计数。没有实现签名/效果的语义增量，也没有更改语言、IR schema、运行时布局或机器优化策略。

## 受影响测试

106项不同测试通过：syntax 55；driver新增CP22 6、既有CP12 12、projects 27、discovery 4、query identity 2。完整命令、源码树和日志SHA见 `TESTS.json` 及相应log。

保留全部环境故障证据：原full-debug尝试ENOSPC；恢复后syntax corpus/driver numberlab依赖缺失产生ENOENT。恢复的17份fixture逐Git blob SHA匹配基线，之后受影响目标通过。没有跳过失败断言或更改测试输入。首次10项通过摘要与最终106项分开记录。

## 真正跨进程CLI

`cli_controls.py` 每次通过 subprocess 新启CLI，不用长驻 CompilationSession。14个独立请求全部满足控制条件，其中9次原生成功执行均每轮 Verify=1，stdout依源码从42变为43。完整stdout JSON、stderr、native stdout及SHA保存在 `cli/`，摘要见 `cli/MANIFEST.json`。

| 请求 | 磁盘语法命中 | 实际解析 | CP12 | Verify |
|---|---:|---:|---|---:|
| cold | 0 | 3 | miss | 1 |
| noop | 3 | 0 | hit | 1 |
| 前模块增加token | 2 | 1 | miss | 1 |
| 私有函数体改变 | 2 | 1 | miss | 1 |
| 公共签名改变 | 2 | 1 | miss | 1 |
| 导入顺序导致ID变化 | 0 | 3 | miss | 1 |
| 重排后noop | 3 | 0 | hit | 1 |
| 损坏3个当前缓存 | 0 | 3 | hit | 1 |
| 文件缺失，缓存/无缓存两请求 | 当前首错一致 | — | bypass/disabled | 0 |
| 文件/目录歧义，缓存/无缓存两请求 | 当前首错一致 | — | bypass/disabled | 0 |
| 修复恢复 | 3 | 0 | hit | 1 |
| 立即取消 | 0 | 0 | bypass | 0 |

缺失/歧义/取消请求均不写语法缓存；之前已发布的可执行文件SHA保持。签名改变时，只减少未改模块的解析；Types仍执行，未宣称跨模块类型缓存复用。

## 构建与可复现身份

严格使用基线同样的 `cargo build --offline --locked --release -p zil-cli`，默认features，CARGO_BUILD_JOBS=2、CARGO_INCREMENTAL=0，独立target `build-cp22`。未加入timings或更改release profile。

最终实际CLI：`/workspace/scratch/4ab361722127/build-cp22/release/zil`；SHA-256 `650e8cd97c0fae262ae8c2a28e3ea6b31532978b9e419c0d2854babfea69373a`。完整命令、编译器版本、文件inode/mtime/ctime、构建日志摘要见 `RELEASE-FINAL.json`。最终binary重复完成相同14请求/9次原生控制，日志在 `cli-final/`，HELP也核对通过。首轮 `RELEASE.json`、`cli/` 证据原样保留。不要移动/重写最终binary后继续使用既有producer缓存测量。

## 验收状态

作者限定正确性检查通过，交独审；正式全workspace/product gates 未运行。CLI controls是正确性序列，不是性能基准，内部seconds不用于提速结论。冷/热完整进程性能由根协调者按冻结对称缓存窗口测量；当前没有吞吐、编译加速或全面优于Rust的量化主张。安全语义验证有上述边界证据；运行性能与表达能力未变；编译收益尚待外部wall/CPU结果。
