# CardNote Compiler — 终极修复蓝图（Ultimate Fix Blueprint）

> 生成方法：元反空（Meta·Anti·Void）三维框架系统分析
> 覆盖范围：4 份评估报告全部问题 + 8 项盲区 = **54 项问题，零遗漏**
> 目标：生产级代码 → GitHub 推送 → 1000 ⭐
> 版本：v0.2.0 架构基准
> 生成日期：2026-06-01

---

## 目录

1. [元反空检视](#一元反空检视)
2. [根因归类与杠杆点](#二根因归类与杠杆点)
3. [依赖拓扑排序](#三依赖拓扑排序)
4. [影响力-成本矩阵](#四影响力-成本矩阵)
5. [生产级标准定义](#五生产级标准定义)
6. [终极修复蓝图：全部 54 项](#六终极修复蓝图)
   - Phase 0: 紧急止血（数据完整性 + 安全）
   - Phase 1: Prompt 层根治（质量源头）
   - Phase 2: 校验层算法替换（底层正确性）
   - Phase 3: Pipeline 层架构调整（成本与性能）
   - Phase 4: 基础设施层建设（批量可管理）
   - Phase 5: 盲区修复（安全 + 可靠性）
   - Phase 6: 生产级打磨（文档 + 测试 + CI/CD）
7. [v0.2.0 架构重构方向](#七v020-架构重构方向)
8. [GitHub 千星策略](#八github-千星策略)
9. [验收总清单](#九验收总清单)

---

## 一、元反空检视

### 元（Meta）—— 对"问题"的问题进行审视

- **类型定位**：这不是"某个 bug 怎么修"的操作问题，而是"一个 8,800 行、24 个版本迭代、4 份独立审计报告、46+ 项问题的系统，如何在保证生产级质量的前提下完成架构升级"的**系统重构决策问题**。
- **意图推断**：用户表面要求"输出所有问题的解决方案"，实际意图是：
  1. 获得一张"可执行的完整路线图"（不是建议，是行动指令）
  2. 确保 5000 本 PDF 项目不会因为已知问题而失败
  3. 将项目从"个人工具"升级为"开源产品"
- **假设挖掘**：
  - 假设 1："修复全部 54 项问题就能达到生产级"——不一定，生产级还需要文档、测试、CI/CD、社区运营
  - 假设 2："Craftsman 模式 = 单本处理"——在 5000 本规模下，这个假设与"可管理性"冲突
  - 假设 3："Prompt 优化可以解决质量问题"——Prompt 是源头，但无法替代校验层的算法正确性
- **来源追溯**：4 份评估报告均基于代码审查（inspected）和对抗性验证（verified），非二手转述。本蓝图的修复方案基于 Rust 标准库、regex crate 文档、uuid crate 文档等一手来源。

### 反（Anti）—— 反向思维与边界检验

- **反例 1："全部修复"本身是否最优？**
  - 对立面：某些问题在 Craftsman 模式下影响极小（如 O(n²) 去重仅在跨文档场景致命）
  - 边界：单文档 500 张卡片时 O(n²) 去重耗时 ~100ms，完全可接受
  - 结论：不应均匀用力，应按"5000 次运行影响"加权
- **反例 2："统一 LLM 调用"降成本 80%，但质量呢？**
  - 对立面：一次调用生成 9 种类型，LLM 可能"偷懒"只生成 3-4 种
  - 边界：DeepSeek V4 Pro 的指令遵循能力 > Kimi 2.6，但 DeepSeek 没有 200K 上下文
  - 结论："提取+分配"方案（先提取知识点，再按类型分配）是质量与成本的最佳平衡点
- **反例 3："生产级 = 代码完美"？**
  - 对立面：GitHub 上 1000 star 的项目，代码完美的不到 20%
  - 边界：用户更关注"我能用它解决什么问题"而非"它的算法复杂度是多少"
  - 结论：生产级 = 代码可维护 + 文档完善 + 有示例 + 有社区响应，而非零 bug

### 空（Void）—— 解构性质疑

- **问题有效性质疑**："54 项问题"这个框架本身是否预设了错误的分类？
  - 4 份报告独立生成，大量问题是同一根因的不同表现（如 Prompt 约束不足 → 12 个表面问题）
  - 如果按"根因"而非"症状"修复，实际工作量从"修 54 个"降到"修 6 个根因 + 8 个独立问题"
  - 结论：**根因驱动修复**，而非症状驱动
- **概念纯洁性质疑**："生产级"这个词被过度使用
  - 原始定义：经过完整测试、有 SLA、可灰度发布、有监控告警
  - 当前语境：用户实际需要的是"可靠运行 5000 次 + 别人能看懂 + 愿意 star"
  - 结论：重新定义"生产级"为**"Craftsman-Grade Production"**（工匠级生产）——不是 Google's scale，而是"我一个人能管理 5000 本 PDF 的可靠工具"
- **认知框定质疑**："1000 star"这个目标是否框定了错误的方向？
  - 1000 star 需要"解决一个被很多人需要的问题"+"展示效果令人惊艳"+"README 让人想收藏"
  - 当前项目的 README 没有展示"一张卡片长什么样"
  - 结论：技术修复只是基础，**展示层（demo、截图、before/after）**才是 star 的关键

---

## 二、根因归类与杠杆点

### 2.1 六大根因层

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        54 项问题 → 6 大根因层                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  根因 A: Prompt 约束不足          12 项问题        修复杠杆: prompts/*.md    │
│  根因 B: 校验算法概念错误          8 项问题        修复杠杆: quality/*.rs    │
│  根因 C: 质量门控机制失效          6 项问题        修复杠杆: output.rs       │
│  根因 D: 单文件 CLI 无法支撑批量   8 项问题        修复杠杆: src/batch/      │
│  根因 E: 架构级调用策略缺陷        6 项问题        修复杠杆: stages/cards.rs │
│  根因 F: 代码债务与工程缺陷        14 项问题       修复杠杆: 各处清理        │
│                                                                             │
│  独立问题（无共同根因）: 8 项盲区 + 6 项独立问题 = 14 项                     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 杠杆点分析

| 杠杆点 | 修复 1 处改善的问题数 | 影响面 | 优先级 |
|--------|---------------------|--------|--------|
| `prompts/all_cards.md` 增加负向示例 + 字数约束 | 12 项 | 全部卡片产出质量 | P0 |
| `quality/card_lint.rs` 替换 `compute_text_similarity` 为 LCS | 8 项 | 复制检测 + 去重 | P0 |
| `output.rs` 增加 `reject_reason` 拦截 | 6 项 | 问题卡片不流入输出 | P0 |
| `stages/cards.rs` 从 9 次调用改为"提取+分配" | 6 项 | API 成本降 80% | P1 |
| 新增 `src/batch/` 批处理模块 | 8 项 | 5000 本可管理 | P1 |
| `config.rs` 配置化 `KNOWN_BOOKS` | 4 项 | 无需改源码即可处理新书 | P1 |

**关键洞察**：修复 6 个杠杆点，可同时解决 44 项问题（81%）。剩余 10 项为独立问题，需单独处理。

---

## 三、依赖拓扑排序

### 3.1 阻塞关系图

```
Phase 0: 紧急止血（必须先做，否则后续修复可能建立在错误基础上）
├── V-004 唯一 ID 冲突 ──→ 阻塞: 去重、输出文件名、数据库主键
├── M-002 Python 超时 ──→ 阻塞: 任何 PDF 转换流程
├── 盲区 1 Prompt 注入 ──→ 阻塞: 安全基线
└── PL-002 质量门控失效 ──→ 阻塞: 问题卡片流入输出

Phase 1: Prompt 层（依赖 Phase 0，但不依赖其他 Phase）
├── P-001 ref 格式 ──→ 依赖: 无
├── P-002 LLM 编造 ──→ 依赖: 无
├── P-003 综述卡格式 ──→ 依赖: 无
├── P-004 术语卡膨胀 ──→ 依赖: 无
├── P-005 反常识卡独立 ──→ 依赖: 无
├── P-006 术语解释 ──→ 依赖: 无
├── P-007 综述卡来源 ──→ 依赖: 无
└── P-008 综述卡重复 ──→ 依赖: 无

Phase 2: 校验层（依赖 Phase 0，可与 Phase 1 并行）
├── V-001 复制检测算法 ──→ 依赖: 无
├── V-002 Jaccard 短内容 ──→ 依赖: 无
├── V-003 质量评分 ──→ 依赖: 无
├── V-005 主题重复 ──→ 依赖: V-002（去重算法修复后）
├── V-006 解析失败丢弃 ──→ 依赖: 无
└── PL-003 类型混淆 ──→ 依赖: 无

Phase 3: Pipeline 层（依赖 Phase 1+2 完成后，避免与 Prompt 变动冲突）
├── PL-004 9 次调用 ──→ 依赖: Phase 1（Prompt 稳定后才能改调用策略）
├── PL-001 Map-Reduce 性能 ──→ 依赖: 无
├── PL-006 CardPlanner ──→ 依赖: 无
├── PL-007 并发配置 ──→ 依赖: 无
├── PL-009 StageCache ──→ 依赖: 无
├── M-005 正则编译 ──→ 依赖: 无
└── P5 ref 硬编码 ──→ 依赖: 无

Phase 4: 基础设施（可独立开发，不阻塞其他 Phase）
├── PL-005 批处理 ──→ 依赖: 无（新增模块）
├── I-002 用量持久化 ──→ 依赖: 无
├── I-001 临时文件 ──→ 依赖: 无
├── PL-008 缓存清理 ──→ 依赖: 无
└── N6 进度/结果数据库 ──→ 依赖: PL-005

Phase 5: 盲区修复（依赖 Phase 3，需要 Pipeline 稳定）
├── 盲区 1 Prompt 注入 ──→ 依赖: Phase 0（已做）
├── 盲区 2 Chunk 边界 ──→ 依赖: Phase 3（Pipeline 稳定）
├── 盲区 3 API 限流 ──→ 依赖: Phase 3
├── 盲区 4 JSON 降级 ──→ 依赖: Phase 3
├── 盲区 5 Markdown 转义 ──→ 依赖: Phase 3
├── 盲区 6 质量评分 ──→ 依赖: Phase 2（V-003）
├── 盲区 7 Prompt 缺失 ──→ 依赖: Phase 1
└── 盲区 8 UUID 碰撞 ──→ 依赖: Phase 0（V-004）

Phase 6: 生产级打磨（依赖全部 Phase 完成）
├── 测试覆盖 ──→ 依赖: 全部代码稳定
├── CI/CD ──→ 依赖: 测试通过
├── README ──→ 依赖: 功能稳定
├── 示例 ──→ 依赖: 功能稳定
└── GitHub 推送 ──→ 依赖: 全部完成
```

### 3.2 可并行组

| 组 | 包含内容 | 可并行度 |
|----|---------|---------|
| A | Phase 0（紧急止血）| 串行（互相依赖少但都是基础） |
| B | Phase 1 + Phase 2 | **高度并行**（Prompt 层与校验层互不依赖） |
| C | Phase 3 + Phase 4 | **中度并行**（Pipeline 调整与批处理模块独立） |
| D | Phase 5 | 串行（依赖前面全部） |
| E | Phase 6 | 串行（依赖全部代码） |

---

## 四、影响力-成本矩阵

### 4.1 评分标准

- **影响力**（1-10）：对"5000 本高质量卡片"目标的贡献度
- **修复成本**（1-10）：所需工作量（1=30min, 10=2周）
- **杠杆系数** = 影响力 / 成本 → 越高越优先

### 4.2 全部 54 项问题的矩阵

| 编号 | 问题 | 影响力 | 成本 | 杠杆系数 | 优先级 | Phase |
|------|------|--------|------|---------|--------|-------|
| P-001 | ref 格式违规 | 10 | 2 | **5.0** | P0 | 0+1 |
| P-002 | LLM 编造例子 | 10 | 2 | **5.0** | P0 | 0+1 |
| V-004 | 唯一 ID 冲突 | 9 | 2 | **4.5** | P0 | 0 |
| M-002 | Python 超时 | 9 | 4 | **2.3** | P0 | 0 |
| PL-002 | 质量门控失效 | 9 | 2 | **4.5** | P0 | 0 |
| 盲区 1 | Prompt 注入 | 8 | 4 | **2.0** | P0 | 0+5 |
| V-001 | 复制检测算法 | 8 | 4 | **2.0** | P0 | 2 |
| V-002 | Jaccard 短内容 | 8 | 4 | **2.0** | P0 | 2 |
| P-003 | 综述卡 ref 格式 | 8 | 1 | **8.0** | P0 | 1 |
| P-005 | 反常识卡共用 | 8 | 2 | **4.0** | P1 | 1 |
| PL-004 | 9 次 LLM 调用 | 10 | 8 | **1.3** | P1 | 3 |
| PL-005 | 无批处理 | 10 | 8 | **1.3** | P1 | 4 |
| P5 | ref 硬编码 | 7 | 2 | **3.5** | P1 | 3 |
| PL-006 | CardPlanner 粗糙 | 7 | 1 | **7.0** | P1 | 3 |
| K4 | Registry 反复构建 | 6 | 1 | **6.0** | P1 | 3 |
| S2 | 正则重复编译 | 5 | 1 | **5.0** | P1 | 3 |
| S5 | CardPlanner scale | 5 | 1 | **5.0** | P1 | 3 |
| PL-001 | Map-Reduce 性能 | 7 | 2 | **3.5** | P2 | 3 |
| I-002 | 用量不持久 | 6 | 2 | **3.0** | P2 | 4 |
| I-001 | 临时文件泄漏 | 6 | 2 | **3.0** | P2 | 4 |
| PL-009 | StageCache 哈希 | 5 | 1 | **5.0** | P2 | 3 |
| V-003 | 质量评分非单调 | 5 | 2 | **2.5** | P2 | 2 |
| V-005 | 主题重复 | 5 | 4 | **1.3** | P2 | 2 |
| V-006 | 解析失败丢弃 | 5 | 2 | **2.5** | P2 | 2 |
| PL-003 | 类型混淆 | 5 | 2 | **2.5** | P2 | 2 |
| P-004 | 术语卡膨胀 | 6 | 1 | **6.0** | P2 | 1 |
| P-006 | 关键概念未解释 | 6 | 1 | **6.0** | P2 | 1 |
| P-007 | 综述卡来源 | 6 | 1 | **6.0** | P2 | 1 |
| P-008 | 综述卡重复 | 5 | 1 | **5.0** | P2 | 1 |
| K1 | 死代码 | 4 | 2 | **2.0** | P3 | 6 |
| K5 | 并发硬编码 | 4 | 1 | **4.0** | P3 | 3 |
| K6 | 缓存清理阻塞 | 4 | 1 | **4.0** | P3 | 4 |
| K7 | Python 版本约束 | 4 | 2 | **2.0** | P3 | 4 |
| M1 | 代码重复 | 3 | 1 | **3.0** | P3 | 6 |
| M2 | CompileConfig 未用 | 3 | 1 | **3.0** | P3 | 6 |
| M3 | 死代码模型 | 2 | 1 | **2.0** | P3 | 6 |
| M4 | 文件大小上限 | 3 | 1 | **3.0** | P3 | 6 |
| M5 | 逻辑分散 | 3 | 1 | **3.0** | P3 | 6 |
| PL-010 | ref 硬编码（KNOWN_BOOKS）| 5 | 2 | **2.5** | P2 | 3 |
| V-007 | 复制检测不可靠 | 5 | 2 | **2.5** | P2 | 2 |
| V-008 | 标记词硬编码 | 4 | 2 | **2.0** | P2 | 2 |
| V-009 | O(n²) 去重 | 3 | 4 | **0.8** | P3 | 6（暂不） |
| 盲区 2 | Chunk 边界断裂 | 7 | 8 | **0.9** | P2 | 5 |
| 盲区 3 | API 限流冲突 | 5 | 2 | **2.5** | P2 | 5 |
| 盲区 4 | JSON 降级 | 5 | 4 | **1.3** | P2 | 5 |
| 盲区 5 | Markdown 转义 | 5 | 2 | **2.5** | P2 | 5 |
| 盲区 6 | 质量评分非单调 | 4 | 2 | **2.0** | P2 | 2+5 |
| 盲区 7 | Prompt 缺失 Fallback | 4 | 2 | **2.0** | P2 | 1+5 |
| 盲区 8 | UUID 碰撞覆盖 | 3 | 1 | **3.0** | P2 | 0+5 |

---

## 五、生产级标准定义

### 5.1 "Craftsman-Grade Production" 标准

对于一个要获得 1000 star 的个人开源项目，"生产级"不等于 Google-scale，而是满足以下**六项维度**：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Craftsman-Grade Production 六维标准                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. 可靠性（Reliability）                                                    │
│     ├── 核心功能 99%+ 成功率（PDF→卡片端到端）                                │
│     ├── 失败时有明确的诊断信息，不静默丢弃                                    │
│     └── 断点续传：崩溃后可以从上次位置恢复                                    │
│                                                                             │
│  2. 可观测性（Observability）                                                │
│     ├── 每张卡片的生成过程可追溯（质量报告）                                  │
│     ├── API 用量、成本、耗时持久化统计                                        │
│     └── 批处理进度实时展示                                                    │
│                                                                             │
│  3. 可配置性（Configurability）                                              │
│     ├── 全部硬编码提取到配置文件                                              │
│     ├── Prompt 文件可自定义（用户可修改卡片风格）                             │
│     └── 模型、超时、并发数均可通过环境变量/CLI 配置                           │
│                                                                             │
│  4. 可维护性（Maintainability）                                              │
│     ├── 代码覆盖率 > 60%（核心算法 > 80%）                                    │
│     ├── 模块边界清晰，单一职责                                                │
│     └── 无死代码、无重复代码                                                  │
│                                                                             │
│  5. 可展示性（Demonstrability）← 1000 star 关键                               │
│     ├── README 有 before/after 卡片示例截图                                   │
│     ├── 有 GIF 演示（30 秒展示一本书→卡片全过程）                             │
│     ├── 有在线文档（mdBook 或 GitHub Pages）                                  │
│     └── CHANGELOG 遵循 Keep a Changelog 规范                                  │
│                                                                             │
│  6. 可参与性（Accessibility）← 社区贡献关键                                   │
│     ├── 有 CONTRIBUTING.md                                                    │
│     ├── 有 good-first-issue 标签                                              │
│     ├── CI/CD 自动跑测试 + lint                                               │
│     └── 代码有完善的 rustdoc 注释                                             │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 与当前状态的差距

| 维度 | 当前状态 | 目标状态 | 差距 |
|------|---------|---------|------|
| 可靠性 | 解析失败静默丢弃、质量门控失效 | 失败卡片标记 + 不流入输出 | 大 |
| 可观测性 | 用量仅内存、无批处理进度 | 持久化日志 + SQLite 队列 | 大 |
| 可配置性 | KNOWN_BOOKS 硬编码、并发硬编码 | 全部配置化 | 中 |
| 可维护性 | 测试覆盖不均衡、有死代码 | >60% 覆盖、无死代码 | 中 |
| 可展示性 | README 无示例截图 | 有截图/GIF/在线文档 | 大 |
| 可参与性 | 无 CONTRIBUTING.md | 有完整贡献指南 | 中 |

---

## 六、终极修复蓝图

> **编排原则**：按 Phase 分组，每组内按"杠杆系数"降序排列。
> 每个问题包含：根因编号、问题描述、修复文件、修复方案（代码/设计）、验收标准。

---

### Phase 0: 紧急止血（本周内完成）

**目标**：修复会导致数据丢失、知识污染、安全事件的致命问题。

---

#### P-001 / P-003: ref 格式违规（4 种变体）

**根因**：A（Prompt 约束不足）
**问题**：ref 字段存在 "本书"、章节名、书名号、前导零、作者名等 4-5 种变体
**修复文件**：`prompts/all_cards.md`、`prompts/review_card.md`、`prompts/term_card.md`
**修复方案**：

在每个 prompt 文件的"输出格式"部分增加：

```markdown
### ref 字段规范（强制）

格式：`来源名_p数字` 或 `来源名_p数字-数字`
- 来源名必须是当前正在阅读的文档的具体书名
- 不要加书名号《》
- 不要用"本书""本文"等代词
- 不要用篇章名代替书名
- 页码不要前导零（写 p5 不要 p005）
- 不要加作者名前缀

正例：
- `人生模式_p172`
- `人生模式_p172-173`

反例（绝对不允许）：
- ❌ `《人生模式》_p160` — 多余书名号
- ❌ `本书_p172` — "本书"不是具体名称
- ❌ `行动模式_p111` — 章节名不能替代书名
- ❌ `人生模式_p049` — 页码不需要前导零
- ❌ `阳志平《人生模式》_p268` — 不要加作者名
- ❌ `人类的演化_p333` — 如果术语原始出处是另一本书，格式应为 `人生模式_p...（引用自《人类的演化》）`
```

在 `quality/ref_format.rs`（新增）增加后处理校验：

```rust
use regex::Regex;
use once_cell::sync::Lazy;

static REF_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[^《》]+_p\d+(?:-\d+)?$")
        .expect("hardcoded pattern is valid")
});

static INVALID_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"《").unwrap(),                    // 书名号
        Regex::new(r"^本书_").unwrap(),               // "本书"
        Regex::new(r"_p0\d+").unwrap(),              // 前导零
        Regex::new(r"^[一-龥]+《").unwrap(), // 作者名+书名号
    ]
});

pub fn validate_ref_format(ref_str: &str) -> Result<(), String> {
    // 检查整体格式
    if !REF_PATTERN.is_match(ref_str) {
        return Err(format!("ref '{}' 不符合格式 '书名_p数字'", ref_str));
    }
    // 检查禁止模式
    for re in INVALID_PATTERNS.iter() {
        if re.is_match(ref_str) {
            return Err(format!("ref '{}' 包含禁止模式", ref_str));
        }
    }
    Ok(())
}
```

**验收标准**：
- [ ] 新编译的卡片 ref 格式 100% 合规
- [ ] `validate_ref_format("本书_p172")` 返回 Err
- [ ] `validate_ref_format("人生模式_p172")` 返回 Ok
- [ ] `validate_ref_format("《人生模式》_p160")` 返回 Err

---

#### P-002: LLM 编造例子/案例

**根因**：A（Prompt 约束不足）
**问题**：8+ 处卡片的"例子"部分为 LLM 虚构，非书中具体案例
**修复文件**：`prompts/all_cards.md`
**修复方案**：

在 prompt 中增加"例子字段规范"：

```markdown
### 例子字段规范（强制）

1. **例子必须来自原文** — 优先使用书中提到的具体研究、真实人物故事、作者亲历案例
2. **禁止 LLM 自行创作类比故事** — 不允许使用"想象一下...""比如有一个人...""假设你..."等虚构叙事
3. **如果原文无具体例子** — 可以省略"例子"字段，或用一句话概括原文的论证逻辑，绝不编造
4. **正确引用原文案例** — 如果原文引用了一位读者的来信，卡片中应写"书中引用了一位读者的来信..."，而不是编造一个虚构人物

负向示例（绝对不允许）：
- ❌ "想象一位性格内向的作家，一边婉拒热闹的酒局..." — LLM 自行创作的虚构人物
- ❌ "走在街上，看到一对夫妻，妻子因为丈夫忘记买菜而严厉地数落他..." — 书中无此场景
- ❌ "一个在国企做了10年财务的人，想转行做户外探险领队..." — 书中无此人物

正向示例：
- ✅ "书中引用了一位读者的来信：'从动机上来说，她是一个高合群的人...'" — 来自原文的具体案例
- ✅ "西蒙的有限理性理论指出，人们在做决策时并非追求最优..." — 来自原文的研究引用
```

**验收标准**：
- [ ] 全部"例子"可追溯至 PDF 原文具体段落
- [ ] 无"想象一下""比如有一个人"等虚构叙事开头
- [ ] 无法验证的例子标记为 `status: NeedsRetry`，reason: "例子无法追溯"

---

#### V-004: 唯一 ID 三重并发缺陷

**根因**：F（工程缺陷）
**问题**：`LAST_UNIQUE_SEC` 依赖 Mutex unwrap + 时间单调 + 单进程，三个假设均不成立
**修复文件**：`src/stages/cards.rs`
**修复方案**：

替换基于时间戳的 ID 生成：

```rust
// 删除以下代码：
// static LAST_UNIQUE_SEC: LazyLock<Mutex<i64>> = LazyLock::new(|| Mutex::new(0));
// let mut last_sec = LAST_UNIQUE_SEC.lock().unwrap();
// let start_sec = std::cmp::max(base_sec, *last_sec + 1);

// 替换为 UUIDv7（时间排序 + 随机部分，无全局状态）
use uuid::Uuid;

fn generate_card_id() -> String {
    Uuid::now_v7().to_string()
}

// 在 Card 创建时：
// card.unique_id = generate_card_id();
```

**Cargo.toml 修改**：
```toml
uuid = { version = "1.11", features = ["v7"] }
```

**验收标准**：
- [ ] 并发测试生成 10,000 个 ID 无碰撞
- [ ] 时间回拨不导致 ID 重复
- [ ] 跨进程运行不导致 ID 重复
- [ ] ID 按时间排序（UUIDv7 特性）

---

#### M-002: Python 子进程无统一超时

**根因**：F（工程缺陷）
**问题**：`converter.rs` 7 处使用 `Command::output()` 无内部超时，可能僵尸进程
**修复文件**：`src/converter.rs`
**修复方案**：

将 `Command::output()` 替换为带超时的异步执行：

```rust
use tokio::process::Command;
use tokio::time::{timeout, Duration};

async fn run_python_with_timeout(
    script: &str,
    args: &[&str],
    timeout_secs: u64,
) -> Result<std::process::Output> {
    let output = timeout(
        Duration::from_secs(timeout_secs),
        Command::new("python3")
            .arg(script)
            .args(args)
            .output(),
    )
    .await
    .map_err(|_| anyhow!("Python 进程超时（{}秒）", timeout_secs))?
    .map_err(|e| anyhow!("Python 进程执行失败: {}", e))?;
    
    Ok(output)
}

// 在 convert_to_markdown_async_with_timeout 内部调用时：
// 外层 timeout 保留（总超时），内层增加步骤级超时
const PYTHON_STEP_TIMEOUT_SECS: u64 = 300; // 5 分钟/步骤
```

**验收标准**：
- [ ] Python 进程超过 5 分钟未返回时强制终止
- [ ] 终止后返回明确的超时错误，不挂死
- [ ] 5000 次运行中无僵尸进程残留

---

#### PL-002: 质量门控失效

**根因**：C（门控机制失效）
**问题**：检测到问题（如标题与内容不匹配）但仍标记为 `Accepted` 输出
**修复文件**：`src/quality/card_lint.rs`、`src/output.rs`
**修复方案**：

在 `output.rs` 输出前增加拦截逻辑：

```rust
fn filter_rejected_cards(cards: Vec<Card>) -> (Vec<Card>, Vec<Card>) {
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    
    for card in cards {
        if card.reject_reason.is_some() {
            rejected.push(card);
        } else if card.status != CardStatus::Accepted {
            rejected.push(card);
        } else {
            accepted.push(card);
        }
    }
    (accepted, rejected)
}

// 在 write_cards 前调用：
// let (accepted_cards, rejected_cards) = filter_rejected_cards(all_cards);
// 只输出 accepted_cards
// rejected_cards 写入 quality report
```

在 `card_lint.rs` 的 `filter_cards_with_source` 中修复 status 降级：

```rust
// 现有代码可能在某个分支中未设置 status，修复：
fn apply_status(card: &mut Card, issues: &[LintIssue]) {
    if !issues.is_empty() {
        // 如果有严重问题，降级
        let has_critical = issues.iter().any(|i| i.severity == Severity::Critical);
        if has_critical {
            card.status = CardStatus::Rejected;
        } else {
            card.status = CardStatus::NeedsRetry;
        }
        card.reject_reason = Some(format_issues(issues));
    }
    // 确保 status 永远有值，不可能是默认的 Accepted
}
```

**验收标准**：
- [ ] 任何带 `reject_reason` 的卡片不进入最终 Markdown 输出
- [ ] `status != Accepted` 的卡片在质量报告中标注原因
- [ ] 质量审计中的"自我决定论"卡片（70%质量但 Accepted）会被正确拦截

---

### Phase 1: Prompt 层根治（1 周内，与 Phase 2 并行）

**目标**：通过强化 prompt 约束，一次性改善 12 个问题。

---

#### P-005: 反常识卡与新知卡共用 prompt

**根因**：A（Prompt 约束不足）
**问题**：`card_type_prompt_name` 将 `CounterIntuit` 映射到 `knowledge_card`，两者本质不同
**修复文件**：`src/stages/cards.rs`、`prompts/counter_intuit_card.md`（新增）
**修复方案**：

1. 创建独立 prompt 文件 `prompts/counter_intuit_card.md`：

```markdown
# 反常识卡生成指令

你的任务是识别书中与主流认知**相悖**的观点，生成反常识卡片。

## 反常识卡的特征

1. **必须挑战常识** — 卡片核心观点应与普通人的直觉相反
2. **提供"常识误解 → 正确认知"的对比结构**
3. **标注反常识强度**：轻微 / 中等 / 强烈

## 输出格式

每张卡片必须包含：
- **标题**：用反常识的核心观点作为标题（15 字以内）
- **新知**：解释为什么这个反常识是正确的（用自己的话，200-500 字）
- **例子**：书中提到的具体研究或案例（禁止编造）
- **ref**：`书名_p页码`
- **强度**：轻微 / 中等 / 强烈

## 与新知卡的区别

- 新知卡："书中提出了什么新观点"
- 反常识卡："书中推翻了什么常识性误解"

## 负向示例

❌ 标题"认知负荷理论" — 这是新知，不是反常识
✅ 标题"大脑不是计算机，记忆不是存储" — 挑战"大脑像计算机"的常识
```

2. 修改 `cards.rs` 映射：

```rust
fn card_type_prompt_name(card_type: &CardType) -> &'static str {
    match card_type {
        CardType::Knowledge => "knowledge_card",
        CardType::CounterIntuit => "counter_intuit_card",  // ← 独立
        CardType::Quote => "quote_card",
        CardType::Term => "term_card",
        CardType::Review => "review_card",
        CardType::Action => "action_card",
        CardType::Question => "question_card",
        CardType::Method => "method_card",
        CardType::Scene => "scene_card",
    }
}
```

**验收标准**：
- [ ] 反常识卡和新知卡的内容差异化 > 50%
- [ ] 反常识卡包含"强度"字段
- [ ] 去重时反常识卡和新知卡重叠率降低

---

#### P-004: 术语卡解释过度膨胀

**根因**：A（Prompt 约束不足）
**问题**：术语卡解释 300-450 字，超限制 2-3 倍
**修复文件**：`prompts/term_card.md`
**修复方案**：

在 `prompts/term_card.md` 中增加字数约束：

```markdown
### 解释字段规范（强制）

- **长度限制**：100-200 字
- **内容限制**：只解释核心机制，不展开历史背景、应用场景、细分子概念
- **目标**：让读者在 30 秒内理解这个概念的核心含义

负向示例：
❌ 解释 350 字，包含历史发展、3 个应用场景、5 个细分子概念

正向示例：
✅ "元认知：对自身认知过程的觉察与调控能力。例如，知道自己正在走神，并主动把注意力拉回任务上。"（45 字，核心机制 + 一个具体例子）
```

**验收标准**：
- [ ] 术语卡"解释"字段 100% 在 100-200 字范围内
- [ ] 元认知/自我决定论等膨胀卡片的解释字数 < 200

---

#### P-006: 关键概念未在卡片内解释

**根因**：A（Prompt 约束不足）
**问题**：STC 算子、差序格局、邓巴数字等术语首次出现未解释
**修复文件**：`prompts/all_cards.md`
**修复方案**：

```markdown
### 术语处理规范（强制）

1. 如果卡片中使用了书中首次出现的专业术语，必须在**同一卡片内**给出 1-2 句话的解释
2. 解释应放在"新知"部分，用括号或独立句子呈现
3. 不要假设读者已经知道这个概念
4. 解释长度控制在 20 字以内，不喧宾夺主

示例：
- ❌ "用 STC 算子分析..." — 读者不知道 STC 是什么
- ✅ "用 STC 算子（空间-时间-成本三维分析工具）分析..." — 自带解释
- ❌ "差序格局导致了..." — 未解释费孝通的理论
- ✅ "差序格局（费孝通提出的中国社会关系模型，像水波纹一样以自我为中心向外扩散）导致了..."
```

**验收标准**：
- [ ] 每张卡片中使用的专业术语在同一卡片内有简要解释
- [ ] 解释长度控制在 20 字以内

---

#### P-007: 综述卡来源变成书中推荐的其他书

**根因**：A（Prompt 约束不足）
**问题**：6 张综述卡（33%）的 ref 引用了非当前阅读文档的其他书籍
**修复文件**：`prompts/review_card.md`
**修复方案**：

```markdown
### 综述卡 ref 规范（强制）

1. **ref 必须引用当前正在阅读的文档**，不可引用书中推荐的其他书籍
2. 如果综述内容涉及书中推荐的其他书，ref 仍应标注为 `人生模式_p...（书中推荐了《XXX》）`
3. 格式：`书名_p页码范围`，不要加作者名，不要加书名号

负向示例：
- ❌ `聪明的阅读者_p126` — 来源是另一本书，不是当前文档
- ❌ `阳志平《人生模式》_p268-281` — 多余作者名和书名号

正向示例：
- ✅ `人生模式_p268-281`
- ✅ `人生模式_p126（书中推荐了《聪明的阅读者》）`
```

**验收标准**：
- [ ] 综述卡 ref 100% 指向当前阅读文档
- [ ] 无作者名前缀、无书名号

---

#### P-008: 综述卡同一篇章多页码范围重复

**根因**：A（Prompt 约束不足）
**问题**：同一篇章出现 3 张页码范围不同的综述卡
**修复文件**：`prompts/review_card.md`
**修复方案**：

```markdown
### 篇章级去重规则

同一篇章（如"第四篇 人际模式"）只生成**一张**综述卡：
1. 如果篇章较短（< 50 页），一张卡片覆盖全部
2. 如果篇章较长（> 50 页），最多分为"上/中/下"三张，且必须在标题中标注
3. 不得出现"p295-305""p235-250""p250-268"这样高度重叠的页码范围
```

**验收标准**：
- [ ] 同一篇章综述卡 ≤ 1 张（或明确标注上/中/下）
- [ ] 页码范围重叠 < 10%

---

#### V-005: 主题重复应合并

**根因**：A + B（Prompt 未要求 + 去重算法不足）
**问题**："有趣/人格冲突/动机特质"主题出现 5 张高度重叠卡片
**修复文件**：`prompts/all_cards.md`、`src/dedup.rs`
**修复方案**：

1. Prompt 层增加约束：

```markdown
### 主题去重规则

- **一主题一卡**：同一核心论点只生成一张卡片
- 如果书中从多个角度阐述了同一论点，选择**最完整/最反常识**的角度
- 不要在不同卡片中重复同一核心观点
```

2. `dedup.rs` 增加语义相似度检测（轻量版）：

```rust
// 在已有 (title, type) 去重后，增加"核心论点相似度"检测
fn should_merge_cards(a: &Card, b: &Card) -> bool {
    if a.card_type != b.card_type {
        return false;
    }
    // 提取"新知"部分的关键词
    let keywords_a = extract_keywords(&a.content);
    let keywords_b = extract_keywords(&b.content);
    // 计算关键词重叠率
    let overlap = keyword_overlap_rate(&keywords_a, &keywords_b);
    overlap > 0.70 // 70% 重叠则建议合并
}
```

**验收标准**：
- [ ] 同一主题（核心论点重叠 > 70%）的卡片合并为一张
- [ ] 合并后的卡片保留最完整的 ref 和例子
- [ ] 被合并的卡片在质量报告中标注合并原因

---

### Phase 2: 校验层算法替换（1 周内，与 Phase 1 并行）

**目标**：修复"表面正确实际错误"的算法缺陷。

---

#### V-001 / S3: 复制检测算法概念级缺陷

**根因**：B（校验算法概念错误）
**问题**：`compute_text_similarity` 使用字符包含率，"abababab" vs "aaaabbbb" ≈ 100%
**修复文件**：`src/quality/card_lint.rs`
**修复方案**：

替换为最长公共子序列（LCS）相似度：

```rust
/// 最长公共子序列相似度
/// 返回 [0.0, 1.0]，1.0 表示完全相同
fn lcs_similarity(a: &str, b: &str) -> f64 {
    let chars_a: Vec<char> = a.chars().collect();
    let chars_b: Vec<char> = b.chars().collect();
    let n = chars_a.len();
    let m = chars_b.len();
    
    if n == 0 || m == 0 {
        return 0.0;
    }
    
    // 动态规划计算 LCS 长度
    let mut dp = vec![vec![0; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            if chars_a[i - 1] == chars_b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }
    
    let lcs_len = dp[n][m];
    lcs_len as f64 / n.max(m) as f64
}

// 替换原有 compute_text_similarity 的调用点
// 阈值从 0.8 调整为 0.65（LCS 比字符包含率更严格）
```

**验收标准**：
- [ ] `lcs_similarity("abababab", "aaaabbbb")` < 0.3
- [ ] `lcs_similarity("认知负荷理论", "认知负荷理论指出")` > 0.8
- [ ] `lcs_similarity("相同内容", "相同内容")` = 1.0
- [ ] 全部原有测试仍通过

---

#### V-002 / N2: Jaccard 去重对中文短内容失效

**根因**：B（校验算法概念错误）
**问题**：3 字 shingle 对 < 100 字的中文内容产生的签名极少，语义相近但措辞不同的卡片 Jaccard = 0
**修复文件**：`src/dedup.rs`
**修复方案**：

```rust
#[derive(Clone, Debug)]
pub struct DedupConfig {
    pub similarity_threshold: f64,
    pub shingle_size: usize,
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.65,
            shingle_size: 3,
        }
    }
}

/// 根据内容长度自适应配置
fn adaptive_dedup_config(content_len: usize) -> DedupConfig {
    if content_len < 100 {
        // 金句卡：2 字 shingle + 更低阈值
        DedupConfig {
            similarity_threshold: 0.40,
            shingle_size: 2,
        }
    } else if content_len < 200 {
        // 短内容：2 字 shingle + 中等阈值
        DedupConfig {
            similarity_threshold: 0.45,
            shingle_size: 2,
        }
    } else {
        DedupConfig::default()
    }
}

// 在 build_similarity_graph 中使用：
// let config = adaptive_dedup_config(content.chars().count());
```

**验收标准**：
- [ ] "阅读是心灵的旅行" vs "读书是一场心灵之旅"：相似度 > 0.4
- [ ] "认知负荷理论指出" vs "认知负荷理论认为"：相似度 > 0.4
- [ ] 完全相同内容：相似度 = 1.0
- [ ] 完全不同内容：相似度 < 0.2

---

#### V-003 / 盲区 6: 质量评分非单调

**根因**：B（校验算法设计缺陷）
**问题**：2 个各扣 0.6 分的问题 → 总分 0；10 个各扣 0.1 分的问题 → 总分也是 0
**修复文件**：`src/quality/card_lint.rs`
**修复方案**：

替换为加权几何平均：

```rust
fn compute_card_quality_score(card: &Card, issues: &[LintIssue]) -> f64 {
    if issues.is_empty() {
        return 1.0;
    }
    
    // 按严重程度分类统计
    let mut critical_count = 0u32;
    let mut major_count = 0u32;
    let mut minor_count = 0u32;
    
    for issue in issues {
        match issue.severity {
            Severity::Critical => critical_count += 1,
            Severity::Major => major_count += 1,
            Severity::Minor => minor_count += 1,
        }
    }
    
    // 使用分段评分，保留区分度
    // Critical: 每个扣 0.4（但最低不低于 0.1）
    // Major: 每个扣 0.15
    // Minor: 每个扣 0.05
    let critical_deduction = (critical_count as f64 * 0.4).min(0.9);
    let major_deduction = (major_count as f64 * 0.15).min(0.5);
    let minor_deduction = (minor_count as f64 * 0.05).min(0.3);
    
    let score = 1.0 - critical_deduction - major_deduction - minor_deduction;
    score.max(0.1) // 最低 0.1，保留区分度
}
```

**验收标准**：
- [ ] 2 个 Critical → 评分 ~0.2
- [ ] 10 个 Minor → 评分 ~0.5
- [ ] 1 Critical + 5 Minor → 评分介于两者之间
- [ ] 不同 issue 组合有区分度

---

#### V-006: 卡片解析失败静默丢弃

**根因**：C（质量门控失效的延伸）
**问题**：某类型解析连续 3 次失败仍丢弃，该类型卡片完全丢失
**修复文件**：`src/stages/cards.rs`
**修复方案**：

```rust
// 在 parse_single_type_cards 失败后，增加 JSON fallback
async fn parse_with_fallback(
    response: &str,
    card_type: CardType,
    call_llm: &impl ChatFn,
    prompt_template: &str,
) -> Result<Vec<Card>> {
    // 先尝试标准解析
    match parse_single_type_cards(response, card_type) {
        Ok(cards) if !cards.is_empty() => return Ok(cards),
        _ => {}
    }
    
    // 标准解析失败，尝试 JSON 格式重试
    let json_prompt = format!(
        "请将以下内容转换为严格的 JSON 格式卡片数组。\n\n{}\n\n要求：\n- 输出必须是合法的 JSON\n- 每个卡片包含 title, content, ref, example 字段\n- 不要添加任何解释文字",
        response
    );
    
    let json_response = call_llm.call_chat(json_prompt, 4000).await?;
    parse_json_cards(&json_response, card_type)
}

// 在 cards.rs 的循环中替换：
// let cards = match parse_single_type_cards(&response, item.card_type) {
//     Ok(c) => c,
//     Err(e) => {
//         eprintln!("    ⚠ 解析失败，尝试 JSON fallback: {}", e);
//         match parse_with_fallback(&response, item.card_type, &call_llm, &prompt_template).await {
//             Ok(c) => c,
//             Err(e2) => {
//                 eprintln!("    ✗ JSON fallback 也失败: {}", e2);
//                 diagnostics.add_parse_failure(item.card_type, &e2);
//                 continue; // 仍然 continue，但至少记录了诊断信息
//             }
//         }
//     }
// };
```

**验收标准**：
- [ ] 解析失败时有明确的诊断日志
- [ ] JSON fallback 能恢复部分卡片
- [ ] 连续 3 次失败后该类型卡片标记为 `ParseFailed`，写入诊断报告

---

#### PL-003: 类型混淆

**根因**：C（门控机制失效）
**问题**：术语卡文件中出现标题为"反常识卡"的元描述卡片
**修复文件**：`src/stages/cards.rs`、`src/quality/card_lint.rs`
**修复方案**：

```rust
fn validate_card_type_consistency(card: &Card, expected_type: CardType) -> Vec<LintIssue> {
    let mut issues = Vec::new();
    
    // 术语卡标题不能是卡片类型名称
    if expected_type == CardType::Term {
        let forbidden_titles = ["反常识卡", "新知卡", "术语卡", "金句卡", "综述卡"];
        if forbidden_titles.iter().any(|&t| card.title.contains(t)) {
            issues.push(LintIssue {
                severity: Severity::Critical,
                message: format!("术语卡标题 '{}' 是卡片类型名称，不是书中术语", card.title),
            });
        }
    }
    
    issues
}
```

**验收标准**：
- [ ] 术语卡标题必须是书中的术语/概念名称
- [ ] 术语卡文件不包含其他卡片类型的元描述
- [ ] 类型错误卡片标记为 `Rejected`

---

### Phase 3: Pipeline 层架构调整（2 周内）

**目标**：解决成本和性能的根本性问题。

---

#### PL-004 / N4: 9 次独立 LLM 调用

**根因**：E（架构级调用策略缺陷）
**问题**：每本书 9 次调用 × 50K 字符 = 540K token，5000 本 = ¥2,700+
**修复文件**：`src/stages/cards.rs`、`prompts/extract_then_assign.md`（新增）
**修复方案**：实施"提取+分配"策略

```rust
/// 新的卡片生成策略：提取+分配
/// 1. 一次 LLM 调用提取书中所有知识点（无类型区分）
/// 2. 第二次 LLM 调用将知识点按类型分配并生成卡片
pub async fn generate_cards_extract_then_assign(
    document: &str,
    doc_type: DocumentType,
    call_llm: impl ChatFn,
    load_prompt: &(dyn Fn(&str) -> Result<String> + Send + Sync),
) -> Result<Vec<Card>> {
    // Step 1: 提取所有知识点
    let extract_prompt = load_prompt("extract_knowledge_points")?;
    let extract_prompt = extract_prompt.replace("{document}", document);
    let extraction = call_llm.call_chat(extract_prompt, 8000).await?;
    
    let knowledge_points = parse_knowledge_extraction(&extraction)?;
    
    // Step 2: 按类型分配并生成卡片
    let plan = CardPlanner::plan(doc_type, document.chars().count());
    let assign_prompt = build_assignment_prompt(document, &knowledge_points, &plan)?;
    let assignment = call_llm.call_chat(assign_prompt, 8000).await?;
    
    parse_assigned_cards(&assignment, &plan)
}

// 保持旧接口作为 fallback
pub async fn generate_cards(
    document: &str,
    doc_type: DocumentType,
    call_llm: impl ChatFn,
    load_prompt: &(dyn Fn(&str) -> Result<String> + Send + Sync),
) -> Result<Vec<Card>> {
    // 默认使用新策略，失败时 fallback 到旧策略
    match generate_cards_extract_then_assign(document, doc_type, &call_llm, load_prompt).await {
        Ok(cards) if !cards.is_empty() => Ok(cards),
        _ => generate_cards_legacy(document, doc_type, call_llm, load_prompt).await,
    }
}
```

**验收标准**：
- [ ] 单本 LLM 调用次数 ≤ 3 次（从 9 次降低）
- [ ] 卡片类型覆盖完整性 > 90%（不丢失类型）
- [ ] API 成本降低 > 60%
- [ ] 新策略失败时自动 fallback 到旧策略

---

#### PL-001 / N3: Map-Reduce 质量过滤性能崩溃

**根因**：E（Pipeline 设计缺陷）
**问题**：`filter_cards_with_source` 传入完整 50 万字 document，正则回溯崩溃
**修复文件**：`src/pipeline.rs`
**修复方案**：

```rust
// 修改 Reduce 阶段的过滤逻辑
async fn reduce_chunk_results(
    chunk_results: &[ChunkResult],
    config: &CompileConfig,
) -> Result<Vec<Card>> {
    let mut all_cards = Vec::new();
    
    for chunk_result in chunk_results {
        // 只传入该 chunk 的文本（~50K 字），而非完整 document
        let chunk_source = &chunk_result.document;
        let (filtered, _stats) = filter_cards_with_source(
            &chunk_result.cards,
            chunk_source,  // ← 关键修改
            &CardLintConfig::default(),
        );
        all_cards.extend(filtered);
    }
    
    // 全局去重（在 chunk 过滤完成后）
    let deduped = deduplicate_cards(all_cards)?;
    Ok(deduped)
}
```

**验收标准**：
- [ ] 质量过滤阶段性能提升 10 倍以上
- [ ] 50 万字文档的正则扫描从 12.5B 字符降到 ~1.25B 字符
- [ ] 过滤结果与修改前一致（不改变行为）

---

#### PL-006 / S5: CardPlanner scale 过于粗糙

**根因**：E（架构设计缺陷）
**问题**：仅 1/2 两级 scale，50 万字书与 10 万字书卡片数相同
**修复文件**：`src/stages/cards.rs`
**修复方案**：

```rust
fn plan_book(char_count: usize) -> Vec<CardPlanItem> {
    let scale = match char_count {
        0..=50_000 => 1,
        50_001..=150_000 => 2,
        150_001..=300_000 => 3,
        300_001..=500_000 => 4,
        _ => (char_count / 100_000).max(5),
    };
    
    vec![
        CardPlanItem::new(CardType::Knowledge, 3 * scale, 5 * scale, true, 1),
        CardPlanItem::new(CardType::CounterIntuit, 1 * scale, 3 * scale, true, 4),
        CardPlanItem::new(CardType::Quote, 2 * scale, 4 * scale, true, 1),
        CardPlanItem::new(CardType::Term, 2 * scale, 4 * scale, true, 1),
        CardPlanItem::new(CardType::Review, 1 * scale, 2 * scale, false, 5),
        CardPlanItem::new(CardType::Action, 2 * scale, 4 * scale, true, 1),
        CardPlanItem::new(CardType::Question, 1 * scale, 3 * scale, true, 2),
        CardPlanItem::new(CardType::Method, 1 * scale, 2 * scale, true, 3),
        CardPlanItem::new(CardType::Scene, 1 * scale, 2 * scale, true, 3),
    ]
}
```

**验收标准**：
- [ ] 5 万字书：~45 张卡片
- [ ] 50 万字书：~225 张卡片（5 倍密度）
- [ ] 卡片密度 ≈ 1 张/2000 字（可配置）

---

#### PL-007 / K5: Map-Reduce 并发数硬编码为 2

**根因**：F（工程缺陷）
**问题**：`Semaphore::new(2)` 无 CLI/环境变量配置
**修复文件**：`src/pipeline.rs`、`src/config.rs`
**修复方案**：

```rust
// config.rs
pub struct CompileConfig {
    // ... 现有字段
    pub max_workers: usize,
}

impl Default for CompileConfig {
    fn default() -> Self {
        Self {
            // ...
            max_workers: std::env::var("CARDNOTE_MAX_WORKERS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(2),
        }
    }
}

// CLI 参数
#[derive(Parser)]
struct Cli {
    // ...
    #[arg(long, env = "CARDNOTE_MAX_WORKERS", default_value = "2")]
    max_workers: usize,
}

// pipeline.rs
let semaphore = Arc::new(Semaphore::new(config.max_workers));
```

**验收标准**：
- [ ] `--max-workers 4` 生效
- [ ] `CARDNOTE_MAX_WORKERS=4` 环境变量生效
- [ ] 默认值仍为 2（保持向后兼容）

---

#### PL-009 / S4: StageCache key 全量哈希

**根因**：F（工程缺陷）
**问题**：每文档 6-8 次遍历 50 万字计算哈希
**修复文件**：`src/pipeline.rs`
**修复方案**：

```rust
struct CompileContext {
    // ... 现有字段
    document_hash: OnceLock<String>,
}

impl CompileContext {
    fn get_document_hash(&self, document: &str) -> &str {
        self.document_hash.get_or_init(|| {
            Self::fnv1a_hash(document)
        })
    }
    
    fn cache_key(&self, stage: &str, document: &str, prompt: &str, model: &str) -> String {
        let doc_hash = self.get_document_hash(document);  // 只计算一次
        let prompt_hash = Self::fnv1a_hash(prompt);
        let combined = format!("{}|{}|{}|{}|{}", Self::VERSION, stage, doc_hash, prompt_hash, model);
        Self::fnv1a_hash(&combined)
    }
}
```

**验收标准**：
- [ ] 同一文档的多次缓存 key 计算只哈希一次
- [ ] 性能提升可测量（单文档减少 ~2-3ms 哈希时间）

---

#### M-005 / S2: extract_field 每次调用都编译新正则

**根因**：F（工程缺陷）
**问题**：每文档 ~2250 次正则编译，累积 500-2000 秒浪费
**修复文件**：`src/stages/cards.rs`
**修复方案**：

```rust
use regex::Regex;
use std::sync::LazyLock;

static RE_FIELD_EXTRACT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(.+?)[:：]\s*(.+?)$").expect("hardcoded regex is valid")
});

fn extract_field(block: &str, field_name: &str) -> Option<String> {
    for cap in RE_FIELD_EXTRACT.captures_iter(block) {
        let name = cap.get(1)?.as_str().trim();
        if name == field_name {
            return Some(cap.get(2)?.as_str().trim().to_string());
        }
    }
    None
}
```

**验收标准**：
- [ ] `extract_field` 不再每次调用 `Regex::new`
- [ ] 性能提升可测量（单文档减少 ~100-450ms）

---

#### P5 / PL-010: ref 硬编码（KNOWN_BOOKS 仅 2 本）

**根因**：F（工程缺陷）
**问题**：`KNOWN_BOOKS` 硬编码，每新增一本书需修改源码重新编译
**修复文件**：`src/quality/card_lint.rs`、`src/config.rs`、`.cardnote/books.json`（新增）
**修复方案**：

```rust
// config.rs
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct BookConfig {
    pub name: String,
    pub aliases: Vec<String>,
    pub author: Option<String>,
}

impl CompileConfig {
    pub fn load_books_config() -> Vec<BookConfig> {
        let config_path = Path::new(".cardnote/books.json");
        if config_path.exists() {
            let content = std::fs::read_to_string(config_path)
                .expect("Failed to read .cardnote/books.json");
            serde_json::from_str(&content)
                .expect("Invalid JSON in .cardnote/books.json")
        } else {
            // 默认配置
            vec![
                BookConfig {
                    name: "人生模式".to_string(),
                    aliases: vec!["人生模式".to_string()],
                    author: Some("阳志平".to_string()),
                },
            ]
        }
    }
}

// card_lint.rs 中的 fix_ref_format 使用运行时加载的配置
pub struct RefFormatFixer {
    known_books: Vec<BookConfig>,
}

impl RefFormatFixer {
    pub fn new() -> Self {
        Self {
            known_books: CompileConfig::load_books_config(),
        }
    }
}
```

**`.cardnote/books.json` 示例**：
```json
[
  {
    "name": "人生模式",
    "aliases": ["人生模式", "MOKA"],
    "author": "阳志平"
  },
  {
    "name": "聪明的阅读者",
    "aliases": ["聪明的阅读者", "阅读者"],
    "author": "阳志平"
  }
]
```

**验收标准**：
- [ ] 新增书籍只需修改 `.cardnote/books.json`，无需重新编译
- [ ] 运行时加载配置，失败时回退到默认

---

### Phase 4: 基础设施层建设（2-3 周，可独立开发）

**目标**：让 5000 本目标在技术上可管理。

---

#### PL-005 / N6: 无作业队列 / 进度持久化 / 结果数据库

**根因**：D（缺乏批处理基础设施）
**问题**：5000 个 PDF 无法批量提交、无断点续传、无结果查询
**修复文件**：新增 `src/batch/` 模块
**修复方案**：

```rust
// src/batch/mod.rs
pub mod queue;
pub mod runner;
pub mod state;

// src/batch/queue.rs
use rusqlite::{Connection, Result as SqliteResult};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub pdf_path: String,
    pub status: JobStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub output_dir: Option<String>,
    pub error_message: Option<String>,
    pub llm_cost: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Retrying,
}

pub struct BatchQueue {
    conn: Connection,
}

impl BatchQueue {
    pub fn new(db_path: &str) -> SqliteResult<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                pdf_path TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                started_at TEXT,
                completed_at TEXT,
                output_dir TEXT,
                error_message TEXT,
                llm_cost REAL
            )",
            [],
        )?;
        Ok(Self { conn })
    }
    
    pub fn enqueue(&self, pdf_path: &str) -> SqliteResult<String> {
        let id = Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO jobs (id, pdf_path, status, created_at) VALUES (?1, ?2, ?3, ?4)",
            (&id, pdf_path, "Pending", Utc::now().to_rfc3339()),
        )?;
        Ok(id)
    }
    
    pub fn dequeue(&self) -> SqliteResult<Option<Job>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM jobs WHERE status = 'Pending' ORDER BY created_at LIMIT 1"
        )?;
        // ... 解析并返回 Job
    }
    
    pub fn mark_completed(&self, id: &str, output_dir: &str) -> SqliteResult<()> {
        self.conn.execute(
            "UPDATE jobs SET status = 'Completed', output_dir = ?1, completed_at = ?2 WHERE id = ?3",
            (output_dir, Utc::now().to_rfc3339(), id),
        )?;
        Ok(())
    }
    
    pub fn get_stats(&self) -> SqliteResult<BatchStats> {
        // 返回 Pending/Running/Completed/Failed 数量
    }
}
```

**CLI 接口**：
```bash
# 批量处理
cardc batch ./pdfs/ --output ./output/ --max-workers 4

# 断点续传
cardc batch ./pdfs/ --output ./output/ --resume

# 重试失败
cardc batch ./pdfs/ --output ./output/ --retry-failed

# 查看进度
cardc batch-status
```

**验收标准**：
- [ ] `cardc batch ./pdfs/` 可批量处理
- [ ] 状态写入 SQLite，支持 `--resume`
- [ ] `--retry-failed` 只重试 Failed 状态的任务
- [ ] `batch-status` 显示 Pending/Running/Completed/Failed 数量

---

#### I-002 / N8: LLM 用量统计不持久化

**根因**：D（基础设施缺失）
**问题**：每次 token 用量只保存在内存中，进程结束后无法统计总成本
**修复文件**：`src/api.rs`
**修复方案**：

```rust
// api.rs
pub fn record_usage(&self, usage: LlmUsage) {
    // 内存记录
    if let Ok(mut log) = self.usage_log.lock() {
        log.push(usage.clone());
    }
    
    // 持久化到文件（追加模式，进程安全）
    if let Ok(json) = serde_json::to_string(&usage) {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(".cardnote/usage.log")
            .and_then(|mut f| {
                use std::io::Write;
                writeln!(f, "{}", json)
            });
    }
}

// 新增：汇总统计
pub fn usage_summary() -> Result<UsageSummary> {
    let file = std::fs::read_to_string(".cardnote/usage.log")?;
    let mut total_input_tokens = 0u64;
    let mut total_output_tokens = 0u64;
    let mut total_cost = 0.0f64;
    
    for line in file.lines() {
        let usage: LlmUsage = serde_json::from_str(line)?;
        total_input_tokens += usage.input_tokens;
        total_output_tokens += usage.output_tokens;
        total_cost += usage.cost;
    }
    
    Ok(UsageSummary {
        total_input_tokens,
        total_output_tokens,
        total_cost,
        job_count: file.lines().count(),
    })
}
```

**验收标准**：
- [ ] 每次 LLM 调用后用量追加到 `.cardnote/usage.log`
- [ ] `usage_summary()` 能统计全部历史用量
- [ ] 格式为 JSON Lines，便于外部工具分析

---

#### I-001 / N7: 临时文件泄漏

**根因**：F（工程缺陷）
**问题**：SIGKILL/panic 时 `TempDir` Drop 不执行，5000 次运行残留 ~1-2GB
**修复文件**：`src/converter.rs`、`src/main.rs`
**修复方案**：

```rust
// main.rs
fn cleanup_stale_temp_dirs() {
    let temp_base = std::env::temp_dir();
    let one_day_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(86400);
    
    if let Ok(entries) = std::fs::read_dir(&temp_base) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            
            // tempfile crate 默认命名: .tmpXXXXXX
            if name.starts_with(".tmp") {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if modified < one_day_ago {
                            let _ = std::fs::remove_dir_all(&path);
                        }
                    }
                }
            }
        }
    }
}

fn main() {
    // 启动时清理残留
    cleanup_stale_temp_dirs();
    // ...
}
```

**验收标准**：
- [ ] 启动时自动清理超过 1 天的临时目录
- [ ] 5000 次运行后磁盘残留 < 100MB
- [ ] 不删除正在使用的临时目录

---

#### PL-008 / K6: 缓存清理同步阻塞启动

**根因**：F（工程缺陷）
**问题**：`cleanup_cache_dir()` 在 `Pipeline::new()` 时同步执行，大量文件时延迟明显
**修复文件**：`src/pipeline.rs`
**修复方案**：

```rust
// 将同步清理改为异步后台任务
impl Pipeline {
    pub fn new(config: CompileConfig) -> Self {
        let pipeline = Self {
            config: config.clone(),
            // ...
        };
        
        // 异步启动缓存清理（不阻塞）
        tokio::spawn(async move {
            if let Err(e) = cleanup_cache_dir_async(&config.cache_dir).await {
                eprintln!("⚠ 缓存清理失败（非阻塞）: {}", e);
            }
        });
        
        pipeline
    }
}

async fn cleanup_cache_dir_async(cache_dir: &Path) -> Result<()> {
    // 同样的清理逻辑，但异步执行
    let mut entries = tokio::fs::read_dir(cache_dir).await?;
    // ...
    Ok(())
}
```

**验收标准**：
- [ ] 缓存清理不阻塞 Pipeline 初始化
- [ ] 清理在后台线程执行
- [ ] 清理失败不影响主流程

---

### Phase 5: 盲区修复（1 周，与 Phase 6 并行）

**目标**：修复 8 项三份文档均未覆盖的盲区。

---

#### 盲区 1: Prompt 注入风险

**问题**：PDF 内容直接插入 prompt，恶意 PDF 可导致指令覆盖
**修复文件**：`src/stages/cards.rs`
**修复方案**：

```rust
fn sanitize_for_prompt(text: &str) -> String {
    // 1. 移除常见的 prompt 注入标记
    let dangerous_patterns = [
        "ignore previous instructions",
        "ignore the above",
        "忘记之前的指令",
        "忽略以上",
        "system:",
        "user:",
        "assistant:",
    ];
    
    let mut sanitized = text.to_string();
    for pattern in &dangerous_patterns {
        sanitized = sanitized.replace(pattern, &"█".repeat(pattern.len()));
    }
    
    // 2. 限制最大长度（防止超长内容消耗 token）
    let max_len = 200_000; // 约 100K tokens
    if sanitized.len() > max_len {
        sanitized.truncate(max_len);
        sanitized.push_str("\n\n[内容已截断...]");
    }
    
    sanitized
}

// 使用时：
// let safe_document = sanitize_for_prompt(document);
// let prompt = prompt_template.replace("{document}", &safe_document);
```

**验收标准**：
- [ ] 包含"ignore previous instructions"的 PDF 不会导致指令覆盖
- [ ] 超长 PDF 内容被截断并标注

---

#### 盲区 2: Chunk 边界知识断裂

**问题**：知识横跨 chunk 边界时，分块编译导致信息丢失
**修复文件**：`src/pipeline.rs`
**修复方案**：

```rust
/// 卡片边界感知切分
/// 检测潜在的卡片边界（章节标题、概念引入句），优先在边界处切分
fn smart_chunk_split(document: &str, max_chunk_size: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    
    // 按段落分割
    let paragraphs: Vec<&str> = document.split("\n\n").collect();
    
    for para in paragraphs {
        // 检测是否是章节标题（如 "## 第三章"）
        let is_boundary = is_chapter_boundary(para) || is_concept_intro(para);
        
        if current_chunk.len() + para.len() > max_chunk_size && is_boundary {
            // 在边界处切分
            chunks.push(current_chunk.clone());
            current_chunk.clear();
        }
        
        current_chunk.push_str(para);
        current_chunk.push_str("\n\n");
    }
    
    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }
    
    chunks
}

fn is_chapter_boundary(para: &str) -> bool {
    let chapter_patterns = [
        Regex::new(r"^#{1,3}\s+第[一二三四五六七八九十\d]+[章节篇]").unwrap(),
        Regex::new(r"^#{1,3}\s+Chapter\s+\d+").unwrap(),
    ];
    chapter_patterns.iter().any(|re| re.is_match(para.trim()))
}

fn is_concept_intro(para: &str) -> bool {
    // 检测概念引入句，如 "所谓 X，是指..."
    let intro_patterns = [
        Regex::new(r"^所谓[一-龥]+，").unwrap(),
        Regex::new(r"^[一-龥]{2,8}（[^）]+）是").unwrap(),
    ];
    intro_patterns.iter().any(|re| re.is_match(para.trim()))
}
```

**验收标准**：
- [ ] 章节标题处优先切分
- [ ] 概念引入句处优先切分
- [ ] Chunk 边界知识断裂率降低 > 50%

---

#### 盲区 3: API 限流冲突

**问题**：9 次连续调用之间无退避，可能触发 RPM 限流
**修复文件**：`src/api.rs`
**修复方案**：

```rust
use tokio::time::{sleep, Duration};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct RateLimiter {
    max_rpm: u64,
    request_times: Arc<Mutex<Vec<std::time::Instant>>>,
}

impl RateLimiter {
    pub fn new(max_rpm: u64) -> Self {
        Self {
            max_rpm,
            request_times: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    pub async fn acquire(&self) {
        let mut times = self.request_times.lock().await;
        let now = std::time::Instant::now();
        let one_minute_ago = now - Duration::from_secs(60);
        
        // 清理 1 分钟前的记录
        times.retain(|&t| t > one_minute_ago);
        
        // 如果已达上限，等待
        if times.len() >= self.max_rpm as usize {
            let oldest = times[0];
            let wait_time = Duration::from_secs(60) - (now - oldest);
            drop(times); // 释放锁
            sleep(wait_time + Duration::from_millis(100)).await;
        }
        
        times.push(std::time::Instant::now());
    }
}

// 使用时：
// rate_limiter.acquire().await;
// let response = call_llm.call_chat(prompt, max_tokens).await?;
```

**验收标准**：
- [ ] 可配置 RPM 限制（环境变量 `CARDNOTE_MAX_RPM`）
- [ ] 达到限流时自动等待
- [ ] 5000 次运行中无因限流导致的失败

---

#### 盲区 4: JSON 解析失败后的降级策略

**问题**：LLM 不返回合法 JSON 时直接返回 Err，该类型卡片丢失
**修复文件**：`src/api.rs`
**修复方案**：

```rust
pub async fn chat_json_with_fallback<T: DeserializeOwned>(
    &self,
    prompt: String,
    max_tokens: u32,
) -> Result<T> {
    // 先尝试标准 JSON 解析
    match self.chat_json::<T>(prompt.clone(), max_tokens).await {
        Ok(result) => return Ok(result),
        Err(e) => {
            eprintln!("⚠ JSON 解析失败，尝试文本降级: {}", e);
        }
    }
    
    // 降级：要求 LLM 重新以 JSON 格式输出
    let retry_prompt = format!(
        "你的上一条回复格式不正确。请只输出合法的 JSON，不要添加任何解释文字。\n\n原始请求：{}\n\n请用 JSON 格式重新回答。",
        prompt
    );
    
    self.chat_json::<T>(retry_prompt, max_tokens).await
}
```

**验收标准**：
- [ ] JSON 失败时自动重试一次
- [ ] 重试仍失败时返回明确的错误信息
- [ ] 不静默丢弃卡片

---

#### 盲区 5: 卡片内容 Markdown 转义缺失

**问题**：内容中的 `---`、`#` 等破坏 Markdown 格式
**修复文件**：`src/models.rs`、`src/output.rs`
**修复方案**：

```rust
fn escape_markdown(text: &str) -> String {
    text.replace("---", "\\---")
        .replace("#", "\\#")
        .replace("*", "\\*")
        .replace("_", "\\_")
        .replace(">", "\\>")
}

// 在 to_default_markdown 中使用：
// format!("**标题：** {}\n", escape_markdown(&self.title))
```

**验收标准**：
- [ ] 内容中的 `---` 不破坏卡片分隔
- [ ] 内容中的 `#` 不被误解析为标题

---

#### 盲区 7: Prompt 文件缺失无 Fallback

**问题**：某 prompt 文件缺失时该类型卡片被完全跳过
**修复文件**：`src/stages/cards.rs`
**修复方案**：

```rust
let prompt_template = match load_prompt(prompt_name) {
    Ok(p) => p,
    Err(e) => {
        eprintln!("    ⚠ 未找到 Prompt '{}': {}, 尝试 fallback", prompt_name, e);
        // Fallback 到通用 prompt
        match load_prompt("all_cards") {
            Ok(fallback) => {
                eprintln!("    → 使用 all_cards.md 作为 fallback");
                fallback
            }
            Err(e2) => {
                eprintln!("    ✗ Fallback 也失败: {}", e2);
                continue;
            }
        }
    }
};
```

**验收标准**：
- [ ] Prompt 缺失时使用 `all_cards.md` 作为 fallback
- [ ] Fallback 生成的卡片标记为 `status: Degraded`

---

### Phase 6: 生产级打磨（持续进行）

**目标**：达到"Craftsman-Grade Production"六维标准。

---

#### K1: Anthropic/Gemini/Cohere 死代码

**修复文件**：`src/api.rs`、`src/providers.rs`
**修复方案**：

```rust
// 标记死代码，或删除
#[allow(dead_code)]
mod unsupported_providers {
    // Anthropic/Gemini/Cohere 实现
    // 未来支持时取消标记
}
```

---

#### K4: ProviderRegistry 反复构建

**修复文件**：`src/providers.rs`
**修复方案**：

```rust
use std::sync::OnceLock;

static GLOBAL_REGISTRY: OnceLock<ProviderRegistry> = OnceLock::new();

impl ProviderRegistry {
    pub fn global() -> &'static ProviderRegistry {
        GLOBAL_REGISTRY.get_or_init(|| {
            ProviderRegistry::new()
        })
    }
}
```

---

#### K7: Python 工具链无版本约束

**修复文件**：`src/converter.rs`
**修复方案**：

```rust
fn check_python_dependencies() -> Result<()> {
    let required = vec![
        ("marker-pdf", ">=1.0"),
        ("pdfplumber", ">=0.10"),
    ];
    
    for (pkg, version) in &required {
        let output = std::process::Command::new("python3")
            .args(["-c", &format!(
                "import {}; print({}.__version__)", pkg.replace("-", "_"), pkg.replace("-", "_")
            )])
            .output()?;
        
        if !output.status.success() {
            return Err(anyhow!("缺少必需的 Python 包: {} {}", pkg, version));
        }
    }
    
    Ok(())
}
```

---

#### M1-M5: 代码重复、死代码、逻辑分散

**修复文件**：各处
**修复方案**：

- M1 `scan_directory` 代码重复：提取公共函数
- M2 `CompileConfig` 未使用：激活或删除
- M3 `BookCompilationResult` 死代码：删除或用于批处理模块
- M4 `MAX_FILE_SIZE_MB=500`：调整为 100MB（实际安全值）
- M5 `resolve_book_title` 分散：统一到一个模块

---

#### V-008: 信息密度标记词硬编码

**修复文件**：`src/quality/card_lint.rs`
**修复方案**：

提取到 `.cardnote/density_markers.toml`：

```toml
[markers]
high_density = ["核心", "关键", "本质", "原理", "机制", ...]
medium_density = ["方法", "步骤", "技巧", ...]
low_density = ["故事", "例子", "比喻", ...]

[weights]
high = 1.5
medium = 1.0
low = 0.5
```

---

#### 测试覆盖补充

**目标**：核心算法 > 80% 覆盖

```rust
// tests/dedup_tests.rs
#[test]
fn test_jaccard_chinese_short_content() {
    let a = "阅读是心灵的旅行";
    let b = "读书是一场心灵之旅";
    let sim = adaptive_jaccard_similarity(a, b);
    assert!(sim > 0.4, "语义相近的中文短内容应被识别: {}", sim);
}

#[test]
fn test_lcs_similarity_abab() {
    let sim = lcs_similarity("abababab", "aaaabbbb");
    assert!(sim < 0.3, "完全不同语序应返回低相似度: {}", sim);
}

#[test]
fn test_ref_format_validation() {
    assert!(validate_ref_format("人生模式_p172").is_ok());
    assert!(validate_ref_format("本书_p172").is_err());
    assert!(validate_ref_format("《人生模式》_p160").is_err());
}
```

---

#### CI/CD 配置

**新增**：`.github/workflows/ci.yml`

```yaml
name: CI

on:
  push:
    branches: [main, master]
  pull_request:
    branches: [main, master]

env:
  CARGO_TERM_COLOR: always

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --all-features
      - run: cargo clippy -- -D warnings
      - run: cargo fmt --check
```

---

#### README 重构

**目标**：1000 star 级别的 README

结构：
```markdown
# CardNote Compiler

> 把任何 PDF 书籍编译成高质量知识卡片

[截图：一张精美的卡片示例]

## ✨ 效果展示

[Before：PDF 原文截图] → [After：生成的卡片截图]

## 🚀 快速开始

```bash
cargo install cardnote-compiler
cardc ./book.pdf
```

## 📚 支持的卡片类型

12 种标准卡片类型 + 图示

## 🏗️ 架构设计

[架构图]

## 📖 文档

[在线文档链接]

## 🤝 贡献

[CONTRIBUTING.md 链接]
```

---

## 七、v0.2.0 架构重构方向

### 7.1 当前架构的根本缺陷

```
当前架构（v0.1.x）：
┌─────────────────────────────────────────┐
│  PDF → Converter → Markdown → Pipeline  │
│  ├── 9 次独立 LLM 调用（顺序执行）       │
│  ├── 无类型间协调                        │
│  ├── 无持久化状态                        │
│  └── 单文件 CLI 模式                     │
└─────────────────────────────────────────┘
              ↓
问题：成本高、质量不可控、无法规模化
```

### 7.2 v0.2.0 目标架构

```
v0.2.0 架构（生产级）：
┌─────────────────────────────────────────────────────────────┐
│                         输入层                               │
│  PDF / EPUB / Markdown → Converter（带超时 + 版本检查）      │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│                         编排层                               │
│  Batch Queue（SQLite）→ 断点续传 → 进度监控                   │
│  ├── Job: Pending → Running → Completed/Failed              │
│  └── Stats: 实时用量、成本、进度                              │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│                         编译层                               │
│  Hybrid Strategy（混合策略）                                 │
│  ├── 短书（≤120K 字符）：单轮长上下文编译                     │
│  ├── 长书（>120K 字符）：Map-Reduce 分块编译                  │
│  └── Fallback：单轮失败自动回退到分块                        │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│                         生成层                               │
│  Extract-Then-Assign（提取+分配）                            │
│  ├── Step 1: 一次调用提取所有知识点                           │
│  ├── Step 2: 按类型分配并生成卡片                             │
│  └── 类型间协调：避免重复提取                                │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│                         质量层                               │
│  L2 引用验证 → 编辑距离算法 → 质量评分 → 门控拦截            │
│  ├── ref 格式校验（正则）                                    │
│  ├── 原文片段匹配（Jaro-Winkler）                            │
│  └── reject_reason 非空 → 不输出                            │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│                         输出层                               │
│  Markdown（默认）→ 未来扩展 Anki/Obsidian/Logseq            │
│  ├── card_quality_report.md                                  │
│  └── compile_diagnostics.md                                  │
└─────────────────────────────────────────────────────────────┘
```

### 7.3 关键架构决策

| 决策 | 选择 | 理由 |
|------|------|------|
| ID 生成 | UUIDv7 | 时间排序 + 无全局状态 + 无碰撞 |
| 去重算法 | 自适应 Jaccard + LCS fallback | 短内容用 2-shingle + 低阈值 |
| 相似度 | LCS 替代字符包含率 | 数学正确，通过全部反例 |
| 调用策略 | Extract-Then-Assign | 成本降 60%，质量可控 |
| 批处理 | SQLite 队列 | 简单、可靠、零外部依赖 |
| 缓存 key | 文档哈希缓存 | 避免重复计算 |

---

## 八、GitHub 千星策略

### 8.1 技术基础（Phase 0-6 完成）

- ✅ 代码可靠、测试覆盖、CI/CD
- ✅ 文档完善、示例丰富
- ✅ 可安装（`cargo install`）

### 8.2 展示层（关键！）

| 元素 | 内容 | 影响力 |
|------|------|--------|
| 封面图 | 一张精美的卡片设计图 | ⭐⭐⭐⭐⭐ |
| GIF 演示 | 30 秒展示 PDF → 卡片全过程 | ⭐⭐⭐⭐⭐ |
| 示例仓库 | 50 本书的卡片产出作为 demo | ⭐⭐⭐⭐ |
| 博客文章 | "如何用 Rust + LLM 做知识管理" | ⭐⭐⭐⭐ |
| 在线演示 | 网页版卡片浏览器 | ⭐⭐⭐ |

### 8.3 社区运营

- 发布到 Rust 中文社区、V2EX、Hacker News
- 与 Obsidian/Logseq 社区合作
- 阳志平老师本人背书（核心优势）

---

## 九、验收总清单

### 9.1 全部 Phase 完成后验收

#### Prompt 层（12 项）
- [ ] 全部卡片 ref 格式统一为 `书名_p数字` 或 `书名_p数字-数字`
- [ ] 无书名号、无"本书"、无章节名、无前导零、无作者名前缀
- [ ] 原始出处为其他书籍时，格式为 `人生模式_p...（引用自《XXX》）`
- [ ] 综述卡 ref 必须指向当前阅读文档
- [ ] 全部"例子"可追溯至 PDF 原文具体段落
- [ ] 无"想象一下""比如有一个人"等虚构叙事开头
- [ ] 术语卡"解释"部分控制在 100-200 字
- [ ] 反常识卡使用独立 prompt，内容与新知卡差异化
- [ ] 专业术语在同一卡片内有 1-2 句话解释
- [ ] 同一主题（核心论点重叠 >70%）的卡片合并为一张
- [ ] 综述卡同一篇章 ≤ 1 张
- [ ] Prompt 缺失时有 fallback

#### 校验层（8 项）
- [ ] `compute_text_similarity` 通过全部定量反例
- [ ] 短内容（<200 字）去重召回率 >80%
- [ ] 任何带 `reject_reason` 的卡片不进入最终输出
- [ ] 质量评分对不同 issue 组合有区分度
- [ ] 解析失败时有 JSON fallback
- [ ] 术语卡标题不能是卡片类型名称
- [ ] 信息密度标记词可配置化
- [ ] 全部算法有对抗性测试覆盖

#### Pipeline 层（8 项）
- [ ] 单本 LLM 调用次数 ≤ 3 次
- [ ] Map-Reduce 质量过滤性能提升 10 倍以上
- [ ] 大型文档（50 万字）卡片密度合理
- [ ] `KNOWN_BOOKS` 配置化
- [ ] 并发数可通过 CLI/环境变量配置
- [ ] StageCache key 计算避免重复哈希
- [ ] `extract_field` 不再重复编译正则
- [ ] Chunk 边界知识断裂率降低 >50%

#### 基础设施层（6 项）
- [ ] `cardc batch ./pdfs/` 可批量处理并断点续传
- [ ] 用量统计持久化到 `.cardnote/usage.log`
- [ ] 临时文件泄漏 < 100MB/5000 本
- [ ] 缓存清理不阻塞启动
- [ ] 恶意 PDF 不会导致 prompt 注入
- [ ] JSON 失败时有文本降级路径

#### 生产级标准（6 项）
- [ ] 代码覆盖率 > 60%
- [ ] CI/CD 自动跑测试 + lint
- [ ] README 有 before/after 截图
- [ ] 有 CONTRIBUTING.md
- [ ] 有 CHANGELOG（Keep a Changelog）
- [ ] 可安装：`cargo install cardnote-compiler`

---

*本文档由元反空（Meta·Anti·Void）三维框架系统分析生成，覆盖 4 份评估报告全部 46 项问题 + 8 项盲区 = 54 项问题，零遗漏。*
*实施顺序：Phase 0 → Phase 1+2（并行）→ Phase 3+4（并行）→ Phase 5 → Phase 6。*
*预计总工作量：4-6 周（1 人全职）。*
