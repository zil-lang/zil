# CP07：配套 Linux LLD 选择

源码提交：`6339ed8b5f2b8250982a3511371e91ad9ba71bc2`。父基线为 CP05–08 派工提交 `6c3b3adb2435137fac9f4a1c8257c44e47d909e8`，继承合流基线 `11e05a70419a5f64191a06f72bc3f16744d01f7b`。

## 行为

- Linux 默认在配置的 Clang 可执行文件同目录找到可执行 `ld.lld` 时，以单个 `--ld-path=<绝对路径>` 参数选中它；未找到则保留原 Clang 链接器选择。支持绝对/相对 Clang 路径与 PATH 中的 Clang。
- `ZIL_LINUX_LINKER=system` 明确保留 Clang 自身默认选择，用于旧链路诊断对照；不保证该默认一定是 GNU ld。
- 其它非空 `ZIL_LINUX_LINKER` 值是一整个可执行路径或 PATH 名称。空值、缺失文件和不可执行文件明确报告 InfraError，不回退。空格和 shell 元字符不会被解释。
- 保留 `ld.lld` 的最终符号链接名字，避免 canonicalize 将其变成通用 `lld` 后改变 LLD 的驱动识别。
- 一次构建只选择一次；仅真实链接命令带参数，单元对象生成不带。API 后端与 Clang 参考后端采用相同选择。
- Win64 完全不读取该 Linux 配置，沿用显式 WindowsLinkOptions。
- 不改变 Verify、语义、ABI、优化 pass、对象缓存键或产物发布协议。

## 实际验证

`author/RESULT.json` 记录源码和测试 SHA256、精确命令、工具版本、日志 SHA256 及限制。固定 LLVM/Clang/LLD 23.1.4、Rust 1.99.1；独立 target、每次 Cargo `-j2`。

启用 LLVM API 的新测试执行15种独立进程配置；无 API 的参考构建执行其中12种。它们是同一套重叠案例，不相加为27个独立功能。检查真实程序退出37、真正调用 LLD 的代理日志、最终 symlink 驱动、两CGU、O0/O2、PATH查找、带空格/分号/美元符的路径、显式系统覆盖、缺配套工具回退、显式坏配置、Clang/API链接失败后的旧可执行文件和旧IR保护。Win64案例经真实 `lld-link` 报缺失 `ExitProcess` 导入，验证Linux配置隔离，不声称Windows机器执行。

已有 backend crate 的39项库测试全部通过；本包 Clippy 全目标 `-D warnings` 与格式检查通过。初次新测试夹具的两个函数共用 FileId，实际只形成一个 CGU，与断言不符；该失败在收据中保留，修复仅令夹具使用不同源文件身份。

本作者未跑性能或整仓正式验收。链接阶段净收益、运行性能与真实 Rust 对比由协调者冻结二进制后串行测量；阶段施工通过不构成产品四目标或突破结论。
