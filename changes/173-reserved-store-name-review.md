# 173 非作者审查返修：Store 保留名预检

/root/package_fields 在 38516c4 的独立审查中发现 `struct Store { x:i32 } fn main(){}` 被 Expand 接受；名义声明和泛型参数预检只接入旧 ByteBuffer 保留名。`type Store=i32` 还能在别名正规化时消去原绑定，使后续 Types 名字核验看不到它。

修复在别名正规化和特化前，将相同窄作用域的三个保留名检查接入 `owned_store_name`。诊断使用已登记 E-OWNED-STORE-NAME；不改变合法成员名，不要求局部值绑定在 Expand 而非既有 Types 阶段拒绝。增加真实源码声明、透明别名、泛型形参和合法成员近邻。该修复落实已发布 Store 保留名合同，没有新增语义权限或降低预算。

原失败探针与后续复审分别保留，独审不得因原59项测试通过忽略此缺口。
