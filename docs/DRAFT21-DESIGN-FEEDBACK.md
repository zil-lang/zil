# draft21交给设计线的具体反馈

2026-10-25（Asia/Shanghai）。099已经独审合入；下列问题不阻塞本轮现有能力，也不在施工中擅自改规则。

## 已落实

ByteBuffer直接拥有Package字段、实际拥有捕获、Package拥有捕获和保存工厂，已贯通源面、独立raw/wire Verify及Linux原生和Win64/Wine兼容执行。内建释放之前先清实际外部责任flag。稳定载体地址并不保证重分配前的payload视图仍有效，这一义务继续保留。

## 需要设计裁决：backing身份是否细化到字段路径

当前窗口来源可保守绑定整个Package shell。好处是整体move、drop、take及helper别名容易统一核验；代价是两个独立ByteBuffer字段也可能被合并为同一backing根。

以下两段直接来自已运行的独审用例，`make()`返回包含独立`a/b/c` ByteBuffer字段的同一个Package；完整定义在[审查源码](../evidence/reviews/099/native_source_review.py)。

```zil
fn value()->i64 {
    let p=make()
    let s=bytes_view(p.b)
    let b=give p.c
    return s.len()+val(b)
}
```

实际结果为Verify阶段`SafeReject / E-BORROW-CONFLICT`。这符合本片批准的保守shell合同，不能当作漏放安全bug；但它阻止一个独立字段的取走。

```zil
fn value()->i64 {
    let p=make()
    let s=bytes_view(p.b)
    let n=s.len()
    let b=give p.c
    return n+val(b)
}
```

最后使用提前后实际`Success`。因此当前限制是活窗期间的来源精度，不能用“借用一直不能结束”描述它。独审还实际覆盖`pair(p.b,p.c)`经两层普通helper写入时的保守冲突。

**请设计线裁决：是否将下一阶段的backing身份提升为“实际拥有载体 + 已核验字段路径”，并让whole move/drop失效其全部后代，而单字段take/replace仅失效对应后代？** 若采用，需同时明确普通Desc参数、capture位置、helper公开apart义务和字段替换后旧视图的含义。不同形参名、字段名或地址恰巧不同均不能自行构成证明。

这一设计不应恢复世代、lease或隐式RC，也不能只在Types中放行而让Verify消费前端许可。独立核需要从实际投影、拥有责任和活性点重建相同关系。现有规范可以保留，待设计给出明确合同后再形成新的跨阶段切片。

## 下一项可并行设计的边界

借入ByteBuffer Desc捕获和从保存环境返回裸slice仍关闭。若要开放，需要把调用期间可能重分配/取走/替换的效果及返回窗口的依托传播到调用者。仅证明闭包环境本身存活不够。拥有错误载荷、一般元素容器及部分初始化也尚未完成；它们应分别有责任和失败合同，不能从u8拥有组合自动推出。

生产状态、接受范围与证据以[字节交接](BYTE-STORAGE-HANDOFF.md)、[099作者结果](../evidence/impl/099/author-result.json)和[099独审](../evidence/reviews/099/review.json)为准。
