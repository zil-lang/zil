这些日志保留施工中真实失败及其限定控制。它们早于最终 clean 提交冻结，不能作为精确提交的验收收据。最终身份以 ../RESULT.json 为准。

224-initial：嵌套 place 误拒与测试夹具插入错误；round4：测试错误假设 Analyze 必须拒绝所有 loan，改为独立 Verify 具体诊断；old-controls：旧 206 的 capability guard；address-controls：旧 171 的 Unsupported→SafeReject 差异；store-controls：共享地址测试硬编码 clang 缺 PATH，shared-controls 为修正环境后原样重跑。171-baseline 使用独立 target 读主树原生产代码做单例对照，未改主树。
