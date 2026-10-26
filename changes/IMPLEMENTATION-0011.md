# 011 运行产物外置

## 原因与行为

仅拆ZIP不能防止每次完整验收重新累积bin与IR。

将大体积运行默认输出迁到out/runs唯一RunId，统一runner显式把所有大子运行收拢本次RunId；保留显式--out-root旧调用兼容，不改变任何实际运行/计数/失败分类/最低门。Node小report通过轻量tests/report.mjs和ZIL_TEST_REPORT_DIR收拢，以wx拒覆盖；handoff专用ZIL_HANDOFF_REPORT继续可用。

统一validate默认只写out，不自动覆写已提交summary；新增显式--publish-summary，复制小结果收据到evidence/validation/<RunId>/result.json并更新final-summary，保留raw_result_path，数值/判定不变。旧check-plan仍钉正式工单evidence显式命令，本片保留兼容边界；小check-docs等既有摘要可暂不动。root随后串行加原生Roam门，本片不替换门。

默认行为只改存放与显式晋升，不删失败、不更改语言验收结论。
