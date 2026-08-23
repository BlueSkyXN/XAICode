# grok-build upstream observation — 2026-08-23

Status: **observed and classified; not integrated**

本记录固定 2026-08-23 的 public source 和 distribution 状态，为下一次 XAICode
incremental migration 提供输入。它不改变 integrated source baseline、产品版本、binary、
Release、安装或运行环境。

## 固定坐标

| 项目 | 值 |
|---|---|
| XAICode integrated upstream | `8a14c91d88875a831a38b3a066b1683116bcb31c` / crate `1.0.0` / `SOURCE_REV` `27b3c66635e2c0bf213429a36ab916f25d59df20` |
| Previous public observation | `be713136d2a69080743a3f6b3c72077057e5948f` / crate `1.0.1` |
| Local `grok-build` before pull | `e5fd4816d43260c15ba785f103990c1ed6cea230` / crate `1.0.3` |
| Public target after `git pull --ff-only` | `19d42e35c07a9c9244f03f6df0c4c353f970d4f9` |
| Target commit time | `2026-08-19T19:55:30Z` |
| Target crate / `SOURCE_REV` | `1.0.6` / `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa` |
| npm stable `latest` | `1.0.5`, published `2026-08-16T00:25:35.078Z`, `gitHead` `5115b46bc909ae5c7f5fc064455197440e796b6b` |
| npm `alpha` | `1.0.8`, published `2026-08-20T21:16:37.994Z`, `gitHead` `95f4d452703b4d0de2b799e3da2667aac509ee82` |
| Source/distribution mapping | `unmapped`; neither npm `gitHead` equals the public target |

Public target is a descendant of both the integrated baseline and previous observation. It is
11 public commits ahead of `8a14c91`, eight ahead of `be71313`, and seven ahead of the local
checkout before this task.

## 三树规模

`python3 scripts/xaicode_maintenance.py audit-upstream --base 8a14c91... --target 19d42e3...`
returned:

| 指标 | 数量 |
|---|---:|
| XAICode clean overlay changed paths | 670 |
| Upstream target changed paths | 1,055 |
| Both changed / semantic-review set | 301 |
| XAICode-only | 369 |
| Upstream-only | 754 |

最大的 overlap area 是 `xai-grok-shell/src` 109、`xai-grok-pager/src` 81、
`xai-grok-workspace/src` 18、pager docs 17、telemetry 12。完整 Git file-stat 为 1,021 files、
211,515 insertions、125,308 deletions。这个批次不是适合裸 merge 或 tree overwrite 的小更新。

## 上游功能摘要与初步分类

下表基于 public source、`1.0.4`–`1.0.6` changelog 和相关实现路径；最终 intake 仍需逐
commit/hunk 审查。

| 功能/变化 | 初步分类 | XAICode 处理 |
|---|---|---|
| `StopCancelled`、turn-end hook reporting | `direct` | 保留 local hook event 和 reason contract，验证取消/拒绝/max-turns 只发一次 |
| `PreToolUse` input rewrite | `adapt` | 有价值，但触碰 tool permission；验证 deny 优先级、rewrite chaining、secret-safe logs |
| `GROK_SESSION_SEARCH` / `[features].session_search` | `direct` | 适合共享 `GROK_HOME`；验证 disabled mode 不建 index、旧 index reopen/concurrency |
| follow-up immediate interjection、goal queue starvation fixes | `adapt` | 保留 XAICode queue/rewind/turn-order contract，重点审查 shell/pager 双侧状态机 |
| text selection、paste、RTL bidi、complete command output | `direct` | 纯本地 TUI；按终端/Unicode边界吸收 |
| optional status line / script output | `adapt` | 仅显式配置；保留 command permission、timeout、ANSI/secret sanitization 和 headless/ACP wire |
| ACP reasoning effort、early stable titles、resume recap | `adapt` | 保留 model identity、local session persistence 和 custom-provider semantics |
| `GROK_CONFIG` / `GROK_CONFIG_PATH` layered overrides | `adapt` | 保留既有 TOML/env precedence；禁止带回 hosted endpoint/account defaults |
| safe worktree auto-GC、git safety/reachability | `adapt` | 高价值但有删除风险；验证 last-copy、dirty/unpushed/ref/build-output、Windows/POSIX |
| `grok clone` content store/projected worktree | `defer` | 需要 CLI naming、network/permission、storage/GC 和 rollback 决策；不能发布 `grok` binary |
| subagent attempt persistence/recovery、ACP lifecycle | `adapt` | 保留 local subagents；逐 stage 审查 persistence codec、rewind、accounting、out-of-order events |
| removal of subagent `capability_mode` | `defer` | `1.0.6` breaking change；先决定 compatibility strategy 和 agent-type tool policy |
| first-launch consent gate/remote consent record | `reject` | 实现读取 remote settings、xAI auth/header 并 POST proxy；保持 production 不可达 |
| `GROK_FORCE_LOGIN_TEAM_ID` | `reject` | 属于 xAI interactive account login，不进入 XAICode production |
| `xai-grok-home`、hosted hub/relay/workspace changes | `reject/adapt` | hosted client 拒绝；仅审查并选择 local filesystem/worktree/runtime slice |
| image/video generation batch limits、ZDR messaging | `defer` | XAICode 当前不启用 first-party media；只有获批 generic implementation 后再评估 |
| web-search domain allow/exclude | `defer` | first-party hosted search 不可达；未来只能用于显式配置的 generic transport |
| external OTLP mTLS/config expansion | `preserve/adapt` | 保留 generic carrier/tests，但不得顺带启用 production exporter 或 vendor sink |
| account/billing/telemetry/upload/updater/npm installer 增量 | `reject` | 维持物理删除或 credential/network 前 fail-closed |

## 建议 intake stage

1. `8a14c91 -> e5fd481`：补齐 public `1.0.1`–`1.0.3`，先重建现有 clean boundary。
2. `e5fd481 -> 5163763`：`1.0.4` hooks、session search、queue/TUI slice。
3. `5163763 -> d92c5b0`：`1.0.5` config layering、worktree GC、ACP reasoning slice。
4. `d92c5b0 -> 19d42e3`：`1.0.6` status line、clone、consent 和 subagent breaking change。

实际 merge 默认按 11 个 public first-parent commits 逐段进行；上面只表示 review/release
边界，不能直接把一个版本范围当作单次无审查 merge。

## 当前结论

- 推荐优先吸收 local hooks/session search/TUI/status/queue/persistence 修复，但必须在新的
  isolated migration worktree 中完成。
- worktree GC、config layering、subagent persistence 是高价值且高 overlap 的适配项，不应
  与 UI-only 变更混成一个无法回滚的 stage。
- `capability_mode` removal 是明确 compatibility decision；consent/team-login/home-hosted/media
  paths 不应因 upstream compilation 依赖被恢复。
- 本观察没有启动源码 migration。下一步入口是
  [`upstream-maintenance.md`](upstream-maintenance.md) 的固定 target、rehearsal、分 stage 和
  exact-head cloud CI 流程。
