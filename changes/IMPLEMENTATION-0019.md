# 0019：共享捕获整合后的夹具与样式迁移

## 依据与范围

统一门 `20261023T041646Z-f5b04c65` 的原始失败保留。016 已支持默认只读共享捕获，两个旧 Unsupported 输入因此成为合法程序；schema 3 又要求每个 SSA ValueId 都有明确的 Hold 行。另有一个嵌套条件触发当前 clippy 门。这些属于配套迁移，不改变语言接受性或验收门。

## 变更

bootstrap 与 CLI/IDE 对拍中的 Unsupported 输入改为写入捕获变量的闭包，继续要求当前未实现的 FnMut 返回 types / Unsupported / E-UNSUPPORTED。原有分类、阶段、诊断一致性、退出码和无输出断言不动。

边界夹具保留原有两个 i32 ValueId，分别填写空 roots 的 ValueHold；普通标量不携带地址来源，但不能省略其密集 Hold 行。核验器仅把等价的嵌套 if 改为 let-chain，不改借入条件、诊断或预算。

## 验证与交接

从本树构建实际 CLI，限定运行 fmt、zil-verify clippy、bootstrap failure-chain / runner-faults、diagnostic-parity 和 check-boundary 的 ct / isolation。记录精确生产提交与原始结果，交非作者独审；root 合入后重跑原统一门。
