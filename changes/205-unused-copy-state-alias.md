# 205：未实例化普通Copy T State的旧别名定义检查

作者与非作者分别用冻结204 CLI05（source7abbbac1f6479c52f4967653acfab74fc56c945d，CLI SHA baa0b5757871b5f7bc31713f61c0a91042d1396087d9bddc65d6872c82a4312d）重放原02。实际生产输入与204正式4fa100e4一致，旧规范92c5身份保留，不倒标新8d6规范。

原源SHA256：`2601332cc0d78658ac96f105e83fea18ce8544bccafe45f2e1ccc8e0254b2d62`。原样输入：

```zil
package State<T,F>{manages bytes:ByteBuffer;a:F;b:F;tag:T}
fn recycle<T:Copy,F:Fn()->i32>(state:State<T,F>)->State<T,F>{
    var next=state
    let view=&next.a
    let replacement=next.a
    next.a=replacement
    view()
    return next
}
fn main(){}

```

实际编译Success、Verify1，但未调用的T:Copy函数被Expand省略，Core只有可达/具体内容；这不是原坏正文得到借用安全证明，更不是已执行的use-after-free。负例未native。作者与独审原结果各在run-logs/205-expand/baseline01与run-logs/205-review-by197/baseline01保留。

集成方授权在既已登记generic_check/generic_places/generic_loop_control/generic_aliases写集中修复该源定义漏检。适用于新205混合入口及实证旧单State定义中的实际借址/别名操作；恢复既有窗口与未用定义规则，不新增安全权限，不改变规范hash。完整State/field真实位置和代次必须保留，take/refill不能复活旧alias，分支/循环/条件和callee-before-arguments均按实际使用检查。无此新操作的旧路径与具名精确费用保持，不能用全局保守拒绝代替last-use/sibling近邻。

原01/03在旧204只遇混合头Unsupported，不能计作alias核通过。修复须用相同字节重新检查02，并以真实last-use、兄弟字段、不冲突转交、take/refill旧epoch和调用准备负例闭合。Types与raw Verify独立入口另核，不能以Expand提前拒绝冒充。完整1.4/B0/产品状态不变。
