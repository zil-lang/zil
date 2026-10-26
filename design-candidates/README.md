# 问号候选，不是当前语言

用户要求先比较全场景语法一致性，暂不裁决。主线保持ZIL-LANG-1.3与A结合：`&T? = (&T)?`。018的前缀实验没有合入生产parser。

文本补丁保留当时的完整源码/规范实验；没有另塞构建产物或复制一套编译器。原始56项限定检查只证明候选解析行为，不是独立接受或Option运行验收。

若需要复现实验，从旁置history bundle恢复main后，在独立worktree使用原基线：

```sh
git worktree add ../zil-option-candidate 0c0de939a6e153e4b546b67a851aec9fff7929c8
git -C ../zil-option-candidate apply /absolute/path/018-prefix-options.patch
```

补丁路径需要替换。不要把补丁直接应用到已经升级共享IR的主线；候选包含当时的语言版本与规范摘要，后续采纳需按当前schema重新整合。完整Git历史仍以主线已接收工作为准，本目录明确记录实验来源和可重建差异。
