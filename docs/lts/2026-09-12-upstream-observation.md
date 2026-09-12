# 上游观察与权限维护切片 — 2026-09-12

Status: **observed and classified; focused downstream fixes; full upstream sync not integrated**

## 范围与固定坐标

本次处理用户要求的本地/云端分支收尾，以及北京时间 9 月 5 日起至 9 月 12 日检查时
的上游变化。初始 XAICode `main` 为 `e4addd85ff04ff6ac111bfb10bd710c7ec2e1776`，
本地与云端一致；仅一个注册工作树、无 stash、无其他分支。PR #2–#7 均已合并，
关闭的 #1 的 `action-gh-release v3` 改动已由 #6 纳入。历史参考快照不是待合并分支。

| 坐标 | 固定值 |
|---|---|
| 已集成 public source | `8a14c91d88875a831a38b3a066b1683116bcb31c`，crate `1.0.0` |
| 已集成 `SOURCE_REV` | `27b3c66635e2c0bf213429a36ab916f25d59df20` |
| 观察目标 | `37949780c144e37df692e3d669051a21fec24f20`，crate `1.0.24` |
| 目标 `SOURCE_REV` | `c4ea71cfdbcdb21e32e41bc25a0043d7d4836714` |
| 目标提交时间 | `2026-09-09T19:03:16Z` |
| npm latest / alpha | `1.0.30`，发布于 `2026-09-11T23:11:16.162Z` |
| npm `gitHead` | `04b7ffed98c6943e3c736e6e8cfb7f979560638e`，与 public source 映射未建立 |
| 本周两个 public sync | `75810042ca2762aa0b0fa17864f3f68823ccbea5`（1,896 个文件），`37949780c144e37df692e3d669051a21fec24f20`（684 个文件） |

用户确认后仅 fetch 现有上游仓库的对象与 remote refs，未 checkout、安装或执行上游代码。
标准维护脚本针对上述固定 refs 的三树审计结果：20 个 public commits，累计上游变化
3,503 个路径、downstream overlay 674 个路径、重叠 585 个、upstream-only 2,918 个。
这些是从已集成 base 起的累计数字，不是本周单独增量。初始 audit 的工作树为干净状态。

## 实际 diff 分类

| slice | 分类 | 判断与后续门槛 |
|---|---|---|
| `rg --hostname-bin` | `adapt`：本次修复 | 本地仅拦 `--pre`，遗漏同样可运行外部程序的 hostname helper；共用参数检测，覆盖 safe list、Auto heuristic 和 broad-grant finding，保留 exact grant |
| Git routine prefix | `adapt`：本次修复 | `checkout`、`switch`、`stash` 整类前缀不能区分正常开发与丢弃工作；改为参数形态判断，保留 shared read-only/exec-risk helpers |
| 上游对 `checkout` 无扩展名参数、`-B/-C` 的放行 | `preserve`：不照搬 | 无扩展名文件与分支有歧义；`-B/-C` 会重置已有分支。本地要求进入现有权限判断，不猜文件扩展名，不直接禁止用户明确授权 |
| `/theme` 别名 picker 匹配 | `direct` 候选、暂缓 | 通用 UI 修复；普通功能未满 7 天观察期。后续单独适配测试，不引入 terminal-theme remote rollout gate |
| mode-000 sandbox spoof 误报 | `defer` | patch 修改 `read_deny_verify` 的 O_PATH/statx 校验；该模块在当前基线不存在。需先审查前置 sandbox slice，不能复制单文件假装适用 |
| 图片错误后的历史 strip | `defer` | diff 从重试成功才改历史变为 Completed/Failed 都可持久化移除；需决定历史保留、备份、rewind/drain-barrier 语义并验证临时持久化 |
| memory v2、attempt 生命周期、工作流 pause/stop | `adapt/defer` | 有本地价值，但涉及新存储和生命周期契约；后续由维护者确定兼容策略与临时 home 回滚测试 |
| MCP 启动 ownership、folder trust 与 instructions/skills | `adapt/defer` | 不能按名称整体拒绝；保留第三方 MCP/OAuth，后续审查启动顺序、已信任项目和 headless 行为 |
| auto-GC/config 文档解析、resume husk 回收 | `adapt/defer` | 涉及删除/回收与 config 语义，需独立验证 dirty/unpushed/last-copy 和 opt-in；本次不运行 GC |
| queue、terminal completion、edit diff 行号、启动缓存/UI | `direct/adapt` 候选、暂缓 | 功能批次初筛，尚未完成全部 hunks 的独立适配；满观察期且有聚焦回归测试后再立项 |
| vendor telemetry、feedback、xAI login 拆分、BotRelay | `preserve/reject` | 不为接受关闭开关/重命名而恢复已删除的 hosted consumer；generic diagnostics 和本地工作流不按名称误删 |
| agent-host/workspaced 和 model-behavior resolver | `defer` | 需区分本地与 hosted 路径，再审查 IPC、provider/model identity 和启动兼容；不随权限修复引入 |

未判定任何现有 clean patch 可 `retire`。本表区分已读重点 patch 与功能初筛；不声称
两个大型 sync 的全部文件都已逐行完成语义审查。其余项继续遵守至少 7 天观察期，
owner 为项目维护者，触发条件为对应表格中的兼容决策、独立 intake 与 exact-head CI。

## 本次实现边界

两项权限缺口直接存在于当前 downstream，因执行型选项误自动授权与潜在数据丢弃风险，
按 LTS 安全/数据损坏修复例外提前处理，不等待普通功能观察期。实现采用当前代码结构
上的聚焦维护补丁，不合并任何 public bulk-sync commit，也不把全部上游变化标记为已吸收。
未来完整同步仍从 `8a14c91` 开始逐段保留真实 Git ancestry，并对本切片做三方适配。

- ripgrep 检测放在已有 `permission/exec_risk.rs`，safe lists 和 heuristic 共用；
  空格/等号参数、wrapper、path-qualified 命令均不能借宽泛授权跳过执行风险判断。
- `--pre-glob` 和普通搜索保持原行为；精确授权仍生效，deny/yolo/分类器优先级不改。
- Git 保留只读查询、add/commit/pull/fetch、明确 switch/new branch 和可恢复 stash 流程；
  force、reset-existing-branch、path checkout、stash drop/clear 与未知参数不走 routine 快路径。
  裸 `checkout name` 无法证明为分支，因此交给现有权限判断；这不是全局禁止该命令。
- 不修改 provider、账号、MCP、telemetry、wire、持久化 schema、依赖或 binary；
  不触碰 live home。已集成 provenance、Cargo/product version 和 tag 保持原样。

## 验证、合并与回滚

本地 gates：`git diff --check`、`cargo fmt --check --all`、`check-contract` 和固定 refs
的 `audit-upstream`；不执行本地 Rust 编译。CI 在原 Linux/macOS composition tests 后增加
`cargo test -p xai-grok-workspace --lib permission::`，实际运行权限回归而不只编译产品入口。
新增/扩展测试覆盖 helper、复合命令、宽泛 policy/session grant 和 exact grant 的差异。

候选必须通过 exact-head CI 与非 tag 的 Release workflow（双 binary smoke、provider
boundary、打包），然后以 merge commit 合入；PR 保存 run URLs 和验证 SHA。合并后复查
main tree 与候选 tree 相同、main CI 成功、无未提交改动或有价值的分支残留。
这些 gates 的完成状态以 PR/Actions 实际记录为准，不由本文的流程描述推断成功。

回滚为 revert 本维护 PR 的 merge commit，不更改已发布 tag。Release workflow 的
非 tag 构建不发布 GitHub Release；本次不安装、部署或宣称真实 provider 使用验收完成。
