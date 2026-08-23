# XAICode LTS 上游维护方法

本方法用于持续观察 `xai-org/grok-build`，并把确认有价值的通用、本地能力以可回滚的
增量方式纳入 XAICode。它借鉴 CPA-Core-LTS 的 protected full-sync 思路，但不照搬其
Go/Panel 契约：XAICode 的核心保护对象是 local-first 数据面、custom provider、持久化
兼容和 Grok/xAI hosted control plane 的不可达性。

## 目标与非目标

目标：

- 持续记录 public source、monorepo `SOURCE_REV`、crate version 和 npm distribution 的
  独立坐标。
- 对每个上游批次做三树审计和语义分类，而不是覆盖 XAICode 源码树。
- 让所有 Rust 编译、测试、binary smoke 和 packaging 只在 GitHub Actions 的精确候选
  SHA 上运行。
- 保持每次 intake 可审查、可停止、可回滚，并明确区分 source、CI、merge、Release、
  installation、runtime 和业务验收。

非目标：

- 不自动 merge、cherry-pick、改 `UPSTREAM.toml`、创建 PR、打 tag 或发布 Release。
- 不用 npm `latest`/`alpha`、crate version 或 changelog 标题代替 exact public commit。
- 不在跟踪任务中恢复 xAI account、billing、relay、hosted workspace、vendor telemetry、
  updater、first-party media/search/storage 等已清理能力。

## 权威工件

| 工件 | 责任 |
|---|---|
| `UPSTREAM.toml` | integrated baseline、latest observed source、distribution metadata、产品版本和 binary policy |
| `CLEAN_BUILD.md` | 当前 clean overlay 和保留/删除边界 |
| `scripts/xaicode_maintenance.py check-contract` | 受保护边界的静态 sentinel；不能替代行为测试 |
| `scripts/xaicode_maintenance.py audit-upstream` | integrated base、XAICode candidate、upstream target 的三树重叠审计 |
| `docs/lts/*-upstream-observation.md` | 某次固定观察的功能分类、风险和 open decision |
| `.github/workflows/upstream-observation.yml` | 定期或手动的只读云端观察；只产出 artifact，不改仓库 |
| `.github/workflows/ci.yml` | 候选 SHA 的 compile、lint、test 和 clean provider boundary 权威结果 |
| `.github/workflows/release.yml` | 双 binary、release profile、artifact 和 tag-triggered Release 权威结果 |

`UPSTREAM.toml [upstream]` 只表示已经集成的来源；`[latest_observed]` 只表示看见了什么，
其中 `record` 指向本次固定 observation。更新后者不授权或暗示已经吸收任何代码。

## 状态分层

| 层级 | 完成条件 |
|---|---|
| Observed | exact public commit、`SOURCE_REV`、crate version、commit time 和 distribution metadata 已记录 |
| Classified | 每个功能批次已有 `direct`、`adapt`、`preserve`、`reject`、`defer` 或 `retire` 结论及依据 |
| Candidate | 隔离 worktree 中完成分 stage 合入，固定 target 不再移动 |
| CI-validated | GitHub Actions successful run 的 `headSha` 与当前 remote branch/PR head 完全一致 |
| Merge-ready | protected seams、回滚、review、required checks、merge method 和 exact head 均已确认 |
| Integrated | merge commit 已进入 `main`，merge tree 与已验证 PR head tree 一致，`main` CI 通过 |
| Released | 授权 tag 与 `product.version` 一致，Release workflow、assets、checksums 和双 binary smoke 已回读 |
| Accepted | 安装、custom provider、临时持久化和真实使用场景分别完成验收；不得由前一层推断 |

## 默认节奏

- 每周由 `upstream-observation.yml` 读取 public `origin/main`；`codex/lts-*` / `codex/upstream-*`
  tracking branch 的相关改动也会触发一次观察，还可以手动输入 40 位 exact commit。
  workflow 使用 `contents: read`，不会 push、开 PR 或执行上游代码。
- 常规功能批次默认至少观察 7 天，并等待风险分类完成后再立项。安全、数据损坏或严重
  回归修复可以缩短观察期，但必须在 intake 记录中写明例外原因。
- 同一时间只维护一个 active upstream intake。新观察可以继续产生，但不能移动正在验证的
  target。
- npm source mapping 不明确时，只能把 npm 作为 distribution evidence；不得声称某个 npm
  label 等价于 public commit。若任务要求 release-to-source 等价而无法建立映射，应停止。

## 分类规则

| 分类 | 含义 | 默认动作 |
|---|---|---|
| `direct` | 通用、本地、无 hosted 依赖，且不改变外部兼容契约 | 按原结构吸收，并保留上游测试 |
| `adapt` | 功能有价值，但触碰 clean boundary、配置、持久化、权限或 wire contract | 只移植必要 slice，补 XAICode 边界测试 |
| `preserve` | XAICode 行为比上游更符合 local-first/LTS 契约 | 保留或在冲突后重放 downstream 行为 |
| `reject` | xAI account/hosted control plane/vendor egress/updater/installer 等产品外能力 | 保留 ancestry evidence，但不进入产品树或保持 production fail-closed |
| `defer` | 需要业务、兼容性、依赖、外部服务或风险接受决策 | 不混入当前 batch，记录 owner/触发条件 |
| `retire` | 上游完整覆盖原 downstream patch 和所有边界测试 | 删除重复 patch，但保留 regression test 与历史记录 |

“上游有相似实现”不构成 `retire`。必须比较完整状态机、错误语义、并发、持久化、权限、
wire shape、跨平台行为和相关 regression tests。

## Protected seams

即使没有文本冲突，下列区域也必须在 PR 中逐项写 `Protected delta review`：

- composition/startup、CLI dispatch、binary names、version/provenance；
- custom provider URL、API/env key、auth scheme、ordinary headers/query、API backend；
- xAI account credential、`auth.json`、login/logout/device flow 和 hosted URL 构造；
- MCP third-party OAuth 与 hosted managed gateway 的区分；
- session JSONL/SQLite/search、compaction、memory、rewind、queue、subagent 和 temporary
  `GROK_HOME` reopen；
- worktree 创建、回收、删除、last-copy/reachability/build-output 安全；
- permissions、hooks input rewrite、tool dispatch、external command/status-line sanitization；
- ACP/protobuf/wire types、reasoning effort、model identity 和 compatibility alias；
- local diagnostics、generic OTLP 与 vendor telemetry/upload 的可达性；
- workspace/local filesystem 与 hosted hub/relay/workspace clients；
- updater、npm launcher、installer、media/search/storage 和任何新增 outbound service。

## Intake 流程

1. **固定观察坐标**
   - 更新现有只读 `grok-build` checkout，记录更新前后 SHA。
   - 读取 target 的 `SOURCE_REV`、`xai-grok-version` crate version、commit time。
   - 用 npm CLI 单独读取 stable/alpha version、publish time、`gitHead`；不推断映射。
   - 先写 observation；此阶段不修改产品源码。

2. **三树审计**
   - Base：`UPSTREAM.toml [upstream].git_commit`。
   - Downstream：最新 `origin/main` 或明确的 XAICode candidate SHA。
   - Target：固定的 exact public upstream commit。
   - 运行 `audit-upstream --list-paths`，按 overlap area 选择需要逐文件审查的 seam。

3. **隔离 rehearsal**
   - 从最新 `origin/main` 建 `codex/upstream-<crate>-<short-sha>` 隔离 worktree。
   - 先做一次完整 rehearsal merge，只用于暴露冲突；不在 root checkout 解决。
   - 按 upstream first-parent public commits 分 stage，默认一个 public sync commit 一段；仅在
     rehearsal 证明低风险时合并相邻 stage。
   - 每段使用真实 Git ancestry。禁止 tree overwrite、`checkout <target> -- .`、squash 或
     rebase 掉 upstream provenance。

4. **语义合入**
   - 先吸收 upstream-only generic/local changes。
   - 对 overlap 文件逐项做 `direct/adapt/preserve/reject/defer/retire` 决策。
   - 每个 stage 后更新 clean sentinels、focused tests 和 observation/PR evidence。
   - 不因编译失败恢复 hosted login、billing、telemetry、remote 或 updater 路径。

5. **仅云端验证**
   - 本地只运行 `check-contract`、`audit-upstream`、`git diff --check` 和
     `cargo fmt --check --all`；不运行会编译 Rust 或创建 `target/` 的 Cargo 命令。
   - push candidate branch 后，以 `.github/workflows/ci.yml` 的 exact-head结果为准。
   - CI 至少覆盖 maintenance、Linux/macOS check+clippy+fmt、composition tests 和 clean
     provider boundary。
   - 在候选 ref 手动运行 `Release` workflow，验证 release profile、两个 binary、CLI smoke、
     provider boundary 和 artifacts；非 tag ref 不发布 GitHub Release。
   - rerun 后必须回读新的 run `headSha`，不能复用旧 green run。

6. **PR 与 merge**
   - PR body 记录 base/target/source rev、每个 stage SHA、冲突文件、分类表、protected delta
     review、CI run URLs、skipped gates、rollback 和未覆盖的运行时验收。
   - upstream sync PR 使用 Create a merge commit；禁止 squash/rebase。
   - merge 前再次确认 PR head 未移动。merge 后确认 `main` tree、main CI、worktree/branch
     residue 和 `UPSTREAM.toml`/docs/version 一致。

7. **Release 与验收**
   - product version 根据 XAICode 行为变化独立决定，不跟随 upstream crate version。
   - 只有明确授权后才创建或移动 `v{product.version}` tag。
   - 回读 Release workflow、artifact names、checksums、`xaicode --version` 和 compatibility
     binary smoke。installation/deployment/runtime 仍需单独授权和验证。

## 云端 gates

| Gate | 必须通过 | 失败处理 |
|---|---|---|
| Observation | exact refs、ancestry、metadata、three-tree artifact | 不开始源码 intake |
| Contract | `check-contract`、forbidden path/symbol、provenance/version | 保留 clean boundary，修最小 source slice |
| Compile/lint | Rust 1.94.0、Linux/macOS `check`、`clippy -D warnings`、`fmt` | 查 exact job log，不退回本地 full build |
| Behavior | composition tests、affected focused tests、custom-provider loopback、zero vendor egress | 停止当前 stage，不能以静态 sentinel 代替 |
| Persistence | copied temporary `GROK_HOME` create/reopen/resume/search/concurrency | 不触碰 live home，不合并 |
| Distribution | release profile、`xaicode` + alias smoke、artifact brand/version | 不打 tag或发布 |

## 回滚与停止条件

- Merge 前：删除 task-owned worktree/branch即可；不改 live data。
- Merge 后、Release 前：revert 对应 sync merge commit，保留审计和失败证据。
- Release 后：使用前一 XAICode tag/asset 回滚；不得移动已发布 tag 来伪装历史。
- 任一阶段出现 custom provider 丢失、hosted path 可达、xAI credential 可能发送到 custom
  endpoint、持久化无兼容路径、binary alias/公共 wire 需要决策、或依赖/服务/数据迁移未获
  授权时立即停止。
- CI green 只证明该 SHA 的云端候选；不能据此声称 merge、Release、installation、runtime
  或真实业务验收完成。
