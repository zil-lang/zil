# 本轮候选源码保全

共同生产基线：`99035931d358add0eaed6d4535035c38413701dd`，tree `1c16744d838064cb1c687ba56ef3a0d17ed3130c`。两份patch均从各自验证工作树的精确已发布源提交对该main生成，使用`git diff --binary --full-index --no-ext-diff --no-color`；未修改生产源码或旧draft patch。

两候选均为NoGo、未合入main；a60的有限screen未证明整体净收益，1679已完整运行质量失败。源码/脚本/正负证据保全不代表采用。下列“远端发布源提交”是本轮已发布源码身份，不宣称退役后仍有同名分支。

作者本地commit与发布commit的完整tree相同，但commit SHA不同；未把同tree写成同commit，也未独立验证作者本地SHA的远端可达性。

| 候选 | 远端发布源提交 | 作者本地提交 | 共同tree | 真实写集 |
|---|---|---|---|---|
| a60 layout投影 | `a60ccf924b43c1ac0ef98b9b0f3d1c270c5789bf` | `89cc46c8ef3781a4a1cd723cbcd4bfff478533f9` | `d260d616a23af8f9c716fbe26c38c200e0eeeb5f` | 2文件 |
| 1679 bounded imports | `1679decf177c9a6eda5a64063c525f06e44e56a2` | `1d29028c637b4af9a880a3db6eb8d678c172f740` | `1e599b1bc2f0f6a0c438088a30faee9ff4018271` | 4文件 |

## a60 layout投影

精确patch：`out/multi-edit/source-diff-a60.patch`；SHA256 `fec98bcf2e4cad46f5a9f6b99207f98f6033e5e300d15605f522d169986050b0`。

写集：

- `crates/zil-backend-llvm/src/machine_cgu/layout.rs`
- `crates/zil-backend-llvm/src/machine_cgu/layout/tests.rs`

最终精确源码独审：`out/multi-edit/source-review-streaming.md`；早期set原型独审`out/multi-edit/source-review.md`仅作历史保留，不替代最终审核。

本地作者对象的`git cat-file -p`元信息（仅本地读取）：

```text
tree d260d616a23af8f9c716fbe26c38c200e0eeeb5f
parent db44395fd05019fab21cb25b878dd1ab6b2199b8
author Codex <codex@openai.com> 1794380417 +0800
committer Codex <codex@openai.com> 1794380417 +0800

Frame GEP symbol streams with the index count first
```

## 1679 bounded imports

精确patch：`out/bounded-imports/source-diff-1679.patch`；SHA256 `b9826c9aafb2b94eadc851d98870fdd17c0b57d3bde3d2edefa67b5bcfdac248`。

写集：

- `crates/zil-backend-llvm/src/machine_cgu/resource_helpers.rs`
- `crates/zil-backend-llvm/src/machine_cgu/resource_helpers/tests.rs`
- `docs/guide/build.md`
- `docs/guide/resource-helper-partition.md`

独立源码复核位置：`out/multi-edit/source-review-bounded-imports.md`。

本地作者对象的`git cat-file -p`元信息（仅本地读取）：

```text
tree 1e599b1bc2f0f6a0c438088a30faee9ff4018271
parent eab1ce3abd0fbda3cc16ea8214d10a24572da2b2
author Codex <codex@openai.com> 1794381874 +0800
committer Codex <codex@openai.com> 1794381874 +0800

Align helper partition documentation with pure-source eligibility
```

本文件及patch进入固定Git archive；raw包按既定边界不包含`.md`或`.patch`。生成工作负载由固定fixture生成器/protocol重建并核hash，不声称全部生成`.zil/.rs`已逐文件另存Git。
