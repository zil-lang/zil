# 语言主线的离线 Git bundle 同步

交付的 `.bundle` 是标准 Git 文件。已有的一键同步脚本可以把它作为本地 fetch 来源；本会话没有该脚本的内容，因而不假设其命令行参数。bundle 内的入口为 **`refs/heads/main`**，指向交付报告及旁置 `.bundle.json` 的完整 `source_commit`。

优先使用**完整 main bundle**：它不要求接收端已经同步到 195，包含交付 main 及其全部可达祖先。增量 bundle 仅供已有指定基线历史的仓库使用，实际先决提交在 manifest 的 `prerequisites` 中；`git bundle verify` 会检查本机是否具备它们。两者都不需要联网。

这份 bundle 不包括无关未合入分支和未提交文件。Git 历史曾经提交的证据仍是相应提交的一部分，保留原提交身份时无法从中剔除；新的执行产物、构建缓存和未跟踪证据不额外装入。精简源码 ZIP 继续遵守零执行产物、零 evidence 的约定。完整 all-ref 审计恢复附件另承担所有可见研发分支的恢复，不把 main-only bundle 描述成全仓分支备份。

## 1. 推荐导入：只取回语言提交

把 bundle 与同名 `.bundle.json` 放在一起。若已有本工具，或者从源码包取出 `tools/language/198/bundle.py`，可以直接运行：

```sh
python3 /path/to/bundle.py --repo /path/to/your-zil import --bundle /path/to/zil-language-main.bundle
```

Windows 安装 Python 3.11 或更新版本时可用：

```powershell
py -3 C:\tools\bundle.py --repo C:\src\zil import --bundle C:\downloads\zil-language-main.bundle
```

路径和文件名换成交付中的实际值。工具依次检查 manifest、SHA256、真实 bundle header 和本地先决提交，再建立 `refs/remotes/zil-language/<完整提交>`。已有正确引用返回 `AlreadyImported`，重复执行不会重复提交；已有同名不同对象则停止。它不 checkout、不 merge、不更改当前分支、index、工作文件或 remote，未提交的编译工作可以留在原位置。对象导入后仍需按下节审查、合并和验证。

如果只使用 Git 原生命令，先按交付报告核对 bundle 的 SHA256，再验证并导入到独立名称。下面的 `<完整提交>` 必须换成交付报告的实际值：

```sh
git bundle verify /path/to/zil-language-main.bundle
git bundle list-heads /path/to/zil-language-main.bundle refs/heads/main
git fetch --no-tags --no-write-fetch-head --recurse-submodules=no /path/to/zil-language-main.bundle refs/heads/main:refs/remotes/zil-language/<完整提交>
git rev-parse refs/remotes/zil-language/<完整提交>
```

检查最后一行与交付报告一致。不使用 `+` 强制 refspec，也不把 bundle 直接 fetch 到本地 `main` 或编译分支。导入工具比这组手工命令多做了不可变引用的原子校验；一键脚本若需要同样的幂等保护，调用工具即可。

## 2. 在单独集成分支合并语言与编译工作

两条支线只会在明确合并后互相看见修改。先在原编译仓库查看 `git status --short`，把需要参与本次合并的修改正常提交。未提交文件不会自动随分支或 bundle 交换；不通过 `reset --hard` 或强制 checkout 消除它们。

推荐新建 worktree，把合并与冲突限制在新集成目录，保留原编译分支。这里的 `compile_branch` 是你本地真实的编译分支名，`language_commit` 从本次 manifest 读取，集成分支与目录需使用尚不存在的名称。

### Git Bash / Linux / WSL

先从原编译仓库运行：

```bash
compile_branch="你的编译分支名"
language_commit="交付报告中的完整提交"
incoming="refs/remotes/zil-language/$language_commit"
git rev-parse --verify "$compile_branch^{commit}"
git rev-parse --verify "$incoming^{commit}"
git worktree add -b integration/language-sync ../zil-language-integration "$compile_branch"
cd ../zil-language-integration
git log --oneline --left-right "HEAD...$incoming"

if git merge-base --is-ancestor HEAD "$incoming"; then
    git merge --ff-only "$incoming"
elif git merge-base --is-ancestor "$incoming" HEAD; then
    printf '%s\n' '当前编译分支已包含这次语言提交。'
elif git merge-base HEAD "$incoming" >/dev/null; then
    git merge --no-ff "$incoming" -m "Merge verified language delivery"
else
    printf '%s\n' '没有共同祖先：停止合并，核对仓库来源或先恢复完整历史。'
fi
```

每条准备命令都应成功后才执行下一条；若 worktree 创建失败，不要继续 `cd` 或合并。最后的合并可能出现正常内容冲突，处理方式见下节。

### Windows PowerShell

下面使用同样的 Git 操作；实际测试环境是 Linux Git，不能把命令语法说明称作已完成原生 Windows 验证。

```powershell
$compileBranch = "你的编译分支名"
$languageCommit = "交付报告中的完整提交"
$incoming = "refs/remotes/zil-language/$languageCommit"
git rev-parse --verify "$compileBranch^{commit}"
if ($LASTEXITCODE -ne 0) { throw "编译分支不存在" }
git rev-parse --verify "$incoming^{commit}"
if ($LASTEXITCODE -ne 0) { throw "先导入语言 bundle" }
git worktree add -b integration/language-sync ..\zil-language-integration $compileBranch
if ($LASTEXITCODE -ne 0) { throw "worktree 未创建，不继续合并" }
Set-Location ..\zil-language-integration
git log --oneline --left-right "HEAD...$incoming"

git merge-base --is-ancestor HEAD $incoming
if ($LASTEXITCODE -eq 0) {
    git merge --ff-only $incoming
} else {
    git merge-base --is-ancestor $incoming HEAD
    if ($LASTEXITCODE -eq 0) {
        Write-Output "当前编译分支已包含这次语言提交。"
    } else {
        git merge-base HEAD $incoming
        if ($LASTEXITCODE -ne 0) { throw "没有共同祖先；先核对来源或恢复完整历史" }
        git merge --no-ff $incoming -m "Merge verified language delivery"
    }
}
if ($LASTEXITCODE -ne 0) { Write-Output "检查 git status，处理冲突或运行 git merge --abort。" }
```

## 3. 冲突、验证和更新原分支

普通分叉会产生一个包含两条历史的 merge commit。冲突时先列出未合并文件：

```sh
git status --short
git diff --name-only --diff-filter=U
```

逐文件保留双方有效改动，再对已解决的文件执行 `git add <文件>`，最后 `git commit`。语言规范、schema、Verify 安全核或编译缓存身份发生冲突时，必须按真实规则合成；不批量选择 `--ours` 或 `--theirs`。如果暂不合并，在这个新集成 worktree 中执行 `git merge --abort`，可回到合并前的编译提交，原编译分支仍保持原位置。

合并成功后，按项目的当前工具链和测试入口验证。完整工作区命令为：

```sh
cargo test --workspace --offline --locked -j2 --no-fail-fast
```

若本机尚未准备离线依赖，先按仓库工具链说明配置环境。Git 合并成功不等于语言与编译优化组合已验证；旧速度测量仍绑定旧基线，不能自动挪到新的语言源码上。

验证后，回到原编译仓库。确认它没有未提交改动，且仍是准备更新的编译分支，再执行：

```sh
git merge --ff-only integration/language-sync
```

该命令只允许快进。如果期间编译分支又新增提交，它会停止；将新增提交在集成分支上明确合并和复测后再继续，不使用强制移动分支。用户若希望两条支线继续保持独立，也可以只保留 `integration/language-sync`，不更新原分支。

没有共同祖先时，可能是接收仓库由源码 ZIP 重新 `git init` 得到，也可能缺少正确恢复历史。保留现有仓库，使用完整 bundle 创建新 checkout 后，把独立的本地改动作为可审查补丁迁移；不通过 `--allow-unrelated-histories` 掩盖身份不一致。

## 4. 导出下一份交付

集成方在干净、冻结的 main checkout 执行：

```sh
python3 tools/language/198/bundle.py export --output ../delivery/zil-language-main.bundle --expect-commit <完整main提交>
```

若双方确定已有 195 的完整提交与祖先，可另导出一个增量，名称必须与完整包不同：

```sh
python3 tools/language/198/bundle.py export --output ../delivery/zil-language-from195.bundle --expect-commit <完整main提交> --base 39459c4428f606ddbbe5a38ca735f2e9ede0609e
```

默认每份 bundle 上限 50,000,000 字节，超限或已有同名 bundle/manifest 时停止，不覆盖旧交付。manifest 明确提交、树、实际先决对象、文件字节数和 SHA256。导出工具仅打包 Git 对象，不重新运行旧语言测试，也不把打包成功当作完整 1.4 或产品终态。
