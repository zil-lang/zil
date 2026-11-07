# 源码、审计与历史交付

当前代码在 `main`，实时任务在独立 `goal:goal.md`，当前语言范围见 [语言初稿](LANGUAGE-DRAFT.md)。本文件说明可复现交付的组成与恢复边界；历史轮次的包装数字和结果按原 Git 提交追溯，不作为当前验收。

## 读取当前源码和计划

```sh
git clone https://github.com/zil-lang/zil.git
cd zil
git fetch origin refs/heads/goal:refs/remotes/origin/goal refs/heads/archive/source-history:refs/remotes/origin/archive/source-history
git show refs/remotes/origin/goal:goal.md
node tools/handoff.mjs --check
```

`archive/source-history` 保存经确认可达的退役源码与阶段 WIP，不合入 main，也不代表验收。`handoff --check` 只查工程记录与历史关系，不读实时 goal、不查全部任务分支、不证明语言安全。源码克隆与 goal 必须分别记录精确 SHA；离线看板注明时点。

## 交付组成

源码 ZIP 按 Git 收录规则导出可构建的源码、规范、测试、工具、当前有效决策及必要精简收据；审计 ZIP 按清单保存被选择的原始运行与失败；Git bundle 保留 main、精确实施/审核历史及交付中声明的其他引用；若要恢复实时计划，须显式包含独立 goal 引用并逐项验证。`PACKAGE.json` 记录来源提交、脏状态、附件名称/哈希和范围；`SHA256SUMS` 绑定源码成员。没有随包交付的 raw、目标动态库或历史 ref，不得宣称可完整重放。已有同名包或运行目录拒绝覆盖。

```sh
python3 tools/package.py ../delivery/zil-source.zip
python3 tools/package_check.py
python3 tools/package_check.py --companions ../delivery
```

包装工具的真实输出决定本次是否含审计包或历史 bundle。`--without-history` 的开发包须明确标缺历史；不能把无 Git 的源码 ZIP 当成可审查旧提交的 checkout。原始运行按需以 `--raw-run out/runs/<领域>/<RunId>` 选择，未选择的运行不得被交付结论引用。不要用 `--write` 改写已交付 manifest 来消除后续不一致。

## 构建与恢复

准备 `rust-toolchain.toml` 要求的 Rust、受支持 LLVM/clang 及与 `Cargo.lock` 匹配的依赖缓存；首次依赖获取可能需要网络。源码构建不依赖审计 ZIP 或 Git 历史：

```sh
cargo fetch --locked
cargo build --workspace --offline --locked
cargo run --offline --locked -p zil-cli -- run examples/modules/main.zil --entry main
```

干净源码克隆在已备好工具链/依赖缓存的环境执行治理检查和适用构建/测试，验证不依赖旧绝对路径或未跟踪本地文件。它不等于空机器从零下载 LLVM、Cargo 依赖或恢复旧运行环境；这些前置与未执行项在收据中单列为 NotRun/Blocked，不伪报 Pass。

若交付包含 `PACKAGE.json` 指定的历史 bundle，从实际附件路径恢复；勿重新 `git init` 冒充原提交：

```sh
git clone --branch main ../zil-source-history.bundle zil-checkout
cd zil-checkout
node tools/handoff.mjs --check
```

bundle 是否携带 goal、任务提交和审核对象须逐 ref 验证，缺少的引用不能由 main 推断。审计附件的核对与恢复：

```sh
python3 tools/evidence_archive.py verify ../zil-source-audit.zip
python3 tools/evidence_archive.py restore ../zil-source-audit.zip --destination .
python3 tools/package_check.py
```

`verify` 无外部预期哈希时只证明 ZIP 内部一致。restore 会检查清单、大小/哈希、重复/大小写冲突、路径逃逸、软链和已有目标；已有相同字节可复用，不覆盖不同字节。需要来源身份时以 `PACKAGE.json` 或明确传入的预期 SHA256 核对。恢复的 raw 仍受原 OS、动态库、工具链及平台范围限制；旧 Fail 不能因恢复或新运行被改写。

## 证据留存

新运行默认放不可覆盖的 `out/runs/<领域>/<RunId>/`。主树只晋升永久协议、精简验收、独立审核与关键失败反例；完整 stdout/stderr、IR、二进制和连续运行按实际交付清单进入审计附件。`docs/decisions/` 保存当前长期决定，任务分支 `.task/` 保存过程，Git 保存历史。旧 main `.task/` 在静态合同和工具迁出并通过验证前留作比对；最终 main 不保留。完整选择/执行/断言、失败字段、exit code 和候选 SHA 由当轮实际收据报告，包装成功不证明测试通过。
