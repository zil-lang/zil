# 337：life-only 普通函数的可变借用返回

规范已由 root 在 `c4da43441b03661e61d0087000a13dbbe7a2a75a` 冻结；
基线生产源码为 `e5a4d51d844e29c4f4f5184eaff5704b1b27dd24`。
本片只实现 `_zil_design.md` profile337，不改变公共 schema/Wire34、ABI、
默认额度或产品门。

## 实际基线与有限差异

源面使用 `&'a mut T`。实际 B CLI 对 life-only 和 CopyT/life 的 mutable
签名均在 Expand 返回 Unsupported。无 life 的 mutable forward、重借、
两实参精确来源、slice tail 和 Copy record 字段投影已有 Verify 成功；
其中四份控制源的 Clang/API × O0/O2 共16次构建/执行均打印42、exit0。
这些是已有能力，不重记为337新增。

本片开放 life-only 普通 Direct 函数及具体 inherent 方法的一层有限
Copy/slice 地址输入和返回。life 标签只声明真实输入位置的来源上界，
共享和独占输入可以同标签。真实 mutable 返回仍须从合法权限产生；
标签不制造权限、apart 或借期缩短。保留 Core binder/dense map、Typed
返回位置上界，Types 检查结构，Analyze 和 Verify 从各自实际 CFG 来源
核验上界，Verify 仍独立重建独占窗口。调用方继续使用实际来源摘要，
未实际返回的伴参不能被声明上界误当成持续借用。

CopyT+mutable life 保持明确 Unsupported。实际既有无 life 未用模板：

```zil
fn bad<T:Copy>(x:&mut T)->&mut T{
    let a=&mut *x;
    let b=&mut *x;
    *a=*b;
    return x
}
fn main(){}
```

在 B 为 Verify Success；同一模板被实例化则 Verify/E-BORROW-CONFLICT。
这是未用定义符号独占检查的 KnownIncomplete，不是已执行源码 UB 的证据，
也不能靠实例化替代定义期证明。原源作为未来回归夹具保留。本片不修改
普通 generic 独占检查，不重做331共享 CopyT/life。

## 交付责任

在既有 cross_function_borrows 目标补真实源/原生、错来源/局部逃逸、共享
升级、重叠实参/窗口及清理近邻；原始 Core 和 Typed/Wire 的声明、权限和
来源伪造须独立拒绝。跨模块、CT 重建、source/live/restored cache 必须重核
声明、实际来源和当前调用窗口。新路径给出累计 N/N−1/fresh，旧精确费用
控制保留；不以删除安全检查或调高默认额度迁移失败。

本文件先于生产编辑提交工作区；后续结果按实际命令和精确提交追加。
完整语言、B0/ZUI、一般 lending/异步和 Rust 四目标仍未完成；没有新增
运行/RSS、维护或编译性能比较，也没有产品级净突破结论。
