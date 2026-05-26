# 42md v0.7.6 技术洞察吸收计划

> 目标：将 42md 反编译发现的全部功能点，结合"PDF为主 + 高质量卡片"需求，按技术难度排优先级。
> 原则：不遗漏任何功能点；不需要AI额度的功能优先吸收；需要AI额度的功能标记为"可选"。

---

## 一、42md 完整功能模块清单（101个功能点）

### 1. 核心编译管道 (compile/)
| # | 模块 | 文件 | 功能描述 |
|---|------|------|---------|
| 1 | 编译主控 | compile/mod.rs | 7阶段编译管道总控 |
| 2 | 文档解析 | compile/parser.rs | 将文档解析为候选卡片(CandidateCard) |
| 3 | 卡片去重 | compile/dedup.rs | 语义去重，输出canonical_title映射 |
| 4 | 卡片质量检查 | compile/card_lint.rs | 检查卡片质量问题，输出LintIssue |
| 5 | 卡片精炼 | compile/card_refine.rs | 根据lint结果修正卡片，输出Correction |
| 6 | 跨卡片综合 | compile/cross_synth.rs | 跨卡片关联分析，发现隐藏关系 |
| 7 | 卡片合成 | compile/synthesize.rs | 最终合成，生成新别名和优化表达 |

### 2. 策展管道 (curate/)
| # | 模块 | 文件 | 功能描述 |
|---|------|------|---------|
| 8 | 策展主流程 | curate/pipeline.rs | 7阶段策展管道（classify→structure→type-specific→golden→intro） |
| 9 | EPUB生成 | curate/epub.rs | 将策展结果打包为EPUB |
| 10 | 后处理 | curate/postprocess.rs | 策展结果的后处理优化 |
| 11 | 书籍编译 | curate/book.rs | 多文档合并为书籍 |
| 12 | 审校 | curate/review.rs | ReviewBookResult: health/stale_items/conflicts/missing_cards/suggestions |

### 3. PDF处理 (pdf/)
| # | 模块 | 文件 | 功能描述 |
|---|------|------|---------|
| 13 | PDF转换 | pdf/convert.rs | lopdf + mutool解析PDF为Markdown |
| 14 | 远程PDF | pdf/remote.rs | 云端PDF处理（OCR/复杂排版） |
| 15 | 页眉页脚处理 | pdf/postprocess/headers.rs | 去除页眉页脚噪声 |
| 16 | LLM辅助PDF | pdf/postprocess/llm.rs | 用LLM修复PDF解析错误 |
| 17 | PDF合并 | pdf/postprocess/merge.rs | 多PDF合并处理 |
| 18 | PDF规则 | pdf/postprocess/rules.rs | 基于规则的PDF清理 |

### 4. 文档解析 (document/)
| # | 模块 | 文件 | 功能描述 |
|---|------|------|---------|
| 19 | Word文档 | document/doc.rs | DOC/DOCX解析 |
| 20 | EPUB | document/epub.rs | EPUB解析 |
| 21 | 本地HTML | document/html_local.rs | HTML文件解析 |
| 22 | Keynote | document/iwork/keynote.rs | Keynote解析 |
| 23 | iWork通用 | document/iwork/mod.rs | Pages/Numbers解析 |
| 24 | MOBI | document/mobi.rs | MOBI/Kindle解析 |
| 25 | ODT | document/odt.rs | OpenDocument解析 |
| 26 | OFD | document/ofd.rs | 版式文档(OFD)解析 |
| 27 | RTF | document/rtf.rs | RTF解析 |
| 28 | 字幕 | document/subtitle.rs | SRT/ASS字幕解析 |
| 29 | Typst | document/typst.rs | Typst源文件解析 |
| 30 | WebArchive | document/webarchive.rs | Safari WebArchive解析 |

### 5. 格式转换器 (converters/)
| # | 模块 | 文件 | 功能描述 |
|---|------|------|---------|
| 31 | 音频转换 | converters/audio.rs | 音频文件处理 |
| 32 | 通用文档 | converters/document.rs | 通用文档转换协调 |
| 33 | 图片转换 | converters/image.rs | 图片OCR/处理 |
| 34 | Markdown | converters/markdown.rs | Markdown标准化 |
| 35 | PDF转换器 | converters/pdf.rs | PDF专用转换逻辑 |
| 36 | URL转换 | converters/url.rs | 网页内容转换 |

### 6. 工具集 (tools/)
| # | 模块 | 文件 | 功能描述 |
|---|------|------|---------|
| 37 | lint主控 | tools/lint/mod.rs | 中文排版优化引擎 |
| 38 | lint规则 | tools/lint/rules.rs | 20+排版规则定义 |
| 39 | lint任务 | tools/lint/task.rs | lint任务执行 |
| 40 | lint类型 | tools/lint/types.rs | 规则类型系统 |
| 41 | lint区域 | tools/lint/zone.rs | 区域化排版处理 |
| 42 | 热词提取 | tools/hotwords/task.rs | 领域热词/术语提取 |
| 43 | AI改进 | tools/improve/mod.rs | AI文本改写优化 |
| 44 | AI改进任务 | tools/improve/task.rs | improve任务执行 |
| 45 | 翻译算法 | tools/translate/algorithm.rs | 翻译策略 |
| 46 | 翻译任务 | tools/translate/task.rs | 翻译执行 |
| 47 | 整站下载 | tools/download/mod.rs | 并发资源下载 |
| 48 | frontmatter | tools/md_export_common/frontmatter.rs | Markdown元数据 |
| 49 | md2docx | tools/md2docx/mod.rs | Markdown转Word |
| 50 | OOXML | tools/md2docx/ooxml.rs | OOXML生成 |
| 51 | md2epub | tools/md2epub/mod.rs | Markdown转EPUB |
| 52 | md2html | tools/md2html/mod.rs | Markdown转HTML |
| 53 | md2pdf引擎 | tools/md2pdf/engine.rs | Typst排版引擎 |
| 54 | PDF图片 | tools/md2pdf/lowering/image.rs | PDF图片嵌入 |
| 55 | PDF降级 | tools/md2pdf/lowering/mod.rs | 内容降级处理 |
| 56 | PDF数学 | tools/md2pdf/math/mod.rs | 数学公式渲染 |
| 57 | md2pdf主控 | tools/md2pdf/mod.rs | Markdown转PDF总控 |
| 58 | PDF远程 | tools/md2pdf/remote.rs | 远程字体/资源 |
| 59 | md2wechat | tools/md2wechat/mod.rs | 公众号HTML |
| 60 | 截图 | tools/screenshot/mod.rs | 网页全页截图 |

### 7. URL处理 (url/)
| # | 模块 | 文件 | 功能描述 |
|---|------|------|---------|
| 61 | 内容提取 | url/extract.rs | 网页正文提取 |
| 62 | HTTP获取 | url/fetch.rs | HTTP请求封装 |
| 63 | robots | url/robots.rs | robots.txt解析 |
| 64 | 站点处理 | url/site.rs | 站点级内容处理 |
| 65 | 资源发现 | url/discovery.rs | 发现页面内资源 |
| 66 | 浏览器控制 | url/browser.rs | CDP浏览器控制 |
| 67 | 适配器通用 | url/adapters/mod.rs | URL适配器框架 |
| 68 | 代码托管 | url/adapters/codehost.rs | GitHub等代码托管 |
| 69 | Dev.to | url/adapters/devto.rs | Dev.to适配 |
| 70 | GitHub | url/adapters/github.rs | GitHub专用适配 |
| 71 | Nature | url/adapters/nature.rs | Nature期刊适配 |
| 72 | StackOverflow | url/adapters/stackoverflow.rs | SO适配 |
| 73 | Wikipedia | url/adapters/wikipedia.rs | 维基百科适配 |

### 8. 图片处理 (image/)
| # | 模块 | 文件 | 功能描述 |
|---|------|------|---------|
| 74 | 预处理 | image/preprocess.rs | 图片预处理（缩放/灰度） |

### 9. 元数据 (metadata/)
| # | 模块 | 文件 | 功能描述 |
|---|------|------|---------|
| 75 | 交叉引用 | metadata/crossref.rs | 文献交叉引用 |

### 10. 任务系统 (task/)
| # | 模块 | 文件 | 功能描述 |
|---|------|------|---------|
| 76 | 转换任务 | task/converter_task.rs | 文档转换任务 |
| 77 | 任务模块 | task/mod.rs | 任务抽象 |
| 78 | 任务管道 | task/pipeline.rs | 任务执行管道 |
| 79 | 注册表 | task/registry.rs | 任务类型注册 |

### 11. 音频识别 (asr/)
| # | 模块 | 文件 | 功能描述 |
|---|------|------|---------|
| 80 | ASR API | asr/api.rs | 语音识别API |
| 81 | 音频格式 | asr/format.rs | 格式转换 |
| 82 | 预处理 | asr/preprocess.rs | 音频预处理 |

### 12. 通用模块 (common/)
| # | 模块 | 文件 | 功能描述 |
|---|------|------|---------|
| 83 | 认证 | common/auth.rs | 设备认证/登录 |
| 84 | 配置 | common/config.rs | 配置管理 |
| 85 | 检测 | common/detect.rs | 文件类型检测 |
| 86 | LLM通用 | common/llm.rs | LLM客户端封装 |
| 87 | 结果同步 | common/result_sync.rs | 云端结果同步 |
| 88 | 文本 | common/text.rs | 文本处理工具 |
| 89 | 上传 | common/upload.rs | 文件上传 |

### 13. 升级系统 (upgrade/)
| # | 模块 | 文件 | 功能描述 |
|---|------|------|---------|
| 90 | 后台升级 | upgrade/background.rs | 后台更新检查 |
| 91 | 版本检查 | upgrade/check.rs | 版本对比 |
| 92 | 安装 | upgrade/install.rs | 自动安装更新 |

### 14. AI轮询 (ai/)
| # | 模块 | 文件 | 功能描述 |
|---|------|------|---------|
| 93 | 轮询 | ai/polling.rs | LLM结果轮询 |

### 15. 服务器路由 (server/routes/)
| # | 模块 | 文件 | 功能描述 |
|---|------|------|---------|
| 94 | 转换路由 | server/routes/convert.rs | /api/convert |
| 95 | 策展路由 | server/routes/curate.rs | /api/compile-book |
| 96 | 发现路由 | server/routes/discover.rs | /api/discover-site |
| 97 | 健康检查 | server/routes/health.rs | /api/health |
| 98 | 知识路由 | server/routes/knowledge.rs | /api/kinds |
| 99 | 媒体路由 | server/routes/media.rs | /api/pdf-pages等 |
| 100 | 任务路由 | server/routes/tasks.rs | /api/tasks/run |
| 101 | 文本工具 | server/routes/text_tools.rs | /api/lint/outline/improve |
| 102 | 翻译路由 | server/routes/translate.rs | /api/translate |

---

## 二、功能点优先级评估

### 评估维度
- **质量影响**：对卡片质量的提升程度（1-5分）
- **技术难度**：实现难度（1-5分，1=极简单，5=极复杂）
- **依赖外部**：是否需要外部服务/工具（高/中/低）
- **AI额度**：是否需要额外LLM调用（是/否）
- **PDF相关**：与PDF处理的相关度（高/中/低）

---

## P0 — 立即实施（质量影响高 + 难度低 + 无AI额度）

### P0-1: 卡片类型扩展（已完成 ✅）
- **42md对应**: compile/mod.rs 中的 CardTypeDesc
- **质量影响**: ★★★★☆ 更多类型=更精确的知识分类
- **技术难度**: ★☆☆☆☆ 已改3个文件
- **AI额度**: 否
- **状态**: 10种类型已扩展，prompt已更新

### P0-2: 分阶段独立重试
- **42md对应**: compile/mod.rs 中的管道设计
- **质量影响**: ★★★★★ 避免因为一个阶段失败而重做所有工作
- **技术难度**: ★★☆☆☆ 已有with_retry，需改为阶段间独立
- **AI额度**: 否（重试不增加总额度消耗）
- **现状**: pipeline.rs 中 compile_chunk 是全有或全无
- **改进**: 摘要成功→实体失败→仅重试实体

### P0-3: 语义卡片去重（替换简单标题去重）
- **42md对应**: compile/dedup.rs
- **质量影响**: ★★★★★ 消除重复卡片是质量最关键一环
- **技术难度**: ★★☆☆☆ 可用文本相似度（simhash/cosine）
- **AI额度**: 否（可用算法实现）
- **现状**: pipeline.rs:706 仅按(title, type)去重
- **改进**: 使用文本嵌入相似度或n-gram匹配

### P0-4: 空卡片/低质量卡片过滤
- **42md对应**: compile/card_lint.rs 中的 LintIssue
- **质量影响**: ★★★★★ 直接消除"空白卡片"问题
- **技术难度**: ★☆☆☆☆ 纯规则判断
- **AI额度**: 否
- **规则**: 内容<50字、标题为空、内容含乱码→丢弃

### P0-5: 卡片引用一致性检查
- **42md对应**: compile/card_lint.rs
- **质量影响**: ★★★★☆ 确保引用与原文匹配
- **技术难度**: ★☆☆☆☆ 字符串匹配
- **AI额度**: 否
- **规则**: 检查"参考"字段是否存在于原文中

### P0-6: PDF解析质量评分增强
- **42md对应**: pdf/postprocess/rules.rs
- **质量影响**: ★★★★☆ 输入质量决定输出质量
- **技术难度**: ★★☆☆☆ 已有quality/metrics.rs基础
- **AI额度**: 否
- **改进**: 增加更多维度（页眉页脚检测、水印检测）

---

## P1 — 短期实施（质量影响高 + 难度中 + 无/低AI额度）

### P1-1: 跨Chunk实体统一
- **42md对应**: compile/cross_synth.rs
- **质量影响**: ★★★★★ 同一术语在不同chunk中应一致
- **技术难度**: ★★★☆☆ 需要全局实体映射表
- **AI额度**: 否
- **现状**: 每个chunk独立编译，实体可能重复/不一致
- **方案**: 收集所有chunk的实体→统一命名→回填

### P1-2: 跨Chunk关系图谱合并
- **42md对应**: compile/cross_synth.rs
- **质量影响**: ★★★★☆ 发现跨chunk的隐藏关系
- **技术难度**: ★★★☆☆ 实体统一后才能合并关系
- **AI额度**: 否
- **现状**: 每个chunk独立生成图谱
- **方案**: 统一实体ID后合并关系，去重

### P1-3: 中文排版优化 (lint)
- **42md对应**: tools/lint/mod.rs + rules.rs + types.rs + zone.rs
- **质量影响**: ★★★★☆ 输出文档可读性大幅提升
- **技术难度**: ★★☆☆☆ 纯规则引擎，20+正则
- **AI额度**: 否
- **规则清单**: 中西文间距、标点挤压、直角引号、数字格式等

### P1-4: PDF后处理（页眉页脚/水印去除）
- **42md对应**: pdf/postprocess/headers.rs + rules.rs
- **质量影响**: ★★★★☆ 减少噪声，提升解析质量
- **技术难度**: ★★★☆☆ 需要分析PDF文本位置模式
- **AI额度**: 否
- **方案**: 检测重复文本块（页眉页脚）、识别水印模式

### P1-5: 策展风格分类
- **42md对应**: curate/pipeline.rs 中的 phase0_classify
- **质量影响**: ★★★★☆ 不同材料类型用不同策略
- **技术难度**: ★★☆☆☆ 基于关键词/结构的规则分类
- **AI额度**: 否（可用规则分类，或用LLM一次性分类）
- **类型**: 专著/传记/文集/小说/诗歌/通俗/手册

### P1-6: 文档类型自动检测
- **42md对应**: common/detect.rs
- **质量影响**: ★★★☆☆ 自动选择最佳解析策略
- **技术难度**: ★★☆☆☆ 基于magic number和扩展名
- **AI额度**: 否
- **现状**: cardc目前只支持PDF，需扩展检测能力

---

## P2 — 中期实施（扩展能力 + 难度中）

### P2-1: Markdown转PDF (md2pdf)
- **42md对应**: tools/md2pdf/mod.rs + engine.rs + lowering/ + math/
- **质量影响**: ★★★☆☆ 输出格式扩展
- **技术难度**: ★★★★☆ Typst集成复杂
- **AI额度**: 否
- **方案**: 集成typst crate或调用typst-cli
- **注意**: typst依赖体积大(40MB+)

### P2-2: Markdown转EPUB (md2epub)
- **42md对应**: tools/md2epub/mod.rs
- **质量影响**: ★★★☆☆ 电子书阅读体验
- **技术难度**: ★★★☆☆ ZIP+XML模板
- **AI额度**: 否
- **方案**: 手写ZIP打包+EPUB3模板

### P2-3: Markdown转HTML (md2html)
- **42md对应**: tools/md2html/mod.rs
- **质量影响**: ★★☆☆☆ 便于网页展示
- **技术难度**: ★★☆☆☆ pulldown-cmark+模板
- **AI额度**: 否
- **方案**: 已有pulldown-cmark，只需加CSS模板

### P2-4: EPUB输入支持
- **42md对应**: document/epub.rs + converters/epub
- **质量影响**: ★★★☆☆ 扩展输入格式
- **技术难度**: ★★★☆☆ ZIP解压+XHTML解析
- **AI额度**: 否
- **方案**: zip crate + roxmltree解析XHTML

### P2-5: DOCX输入支持
- **42md对应**: document/doc.rs + md2docx/
- **质量影响**: ★★★☆☆ 扩展输入格式
- **技术难度**: ★★★☆☆ OOXML解析
- **AI额度**: 否
- **方案**: docx-rs crate读取

### P2-6: HTML输入支持
- **42md对应**: document/html_local.rs + url/extract.rs
- **质量影响**: ★★★☆☆ 支持保存的网页
- **技术难度**: ★★☆☆☆ html5ever/scraper提取正文
- **AI额度**: 否

### P2-7: URL输入支持（基础版）
- **42md对应**: url/extract.rs + url/fetch.rs
- **质量影响**: ★★★☆☆ 直接编译网页
- **技术难度**: ★★★☆☆ 需要正文提取算法
- **AI额度**: 否（HTTP获取+HTML解析）
- **方案**: reqwest获取+readability算法提取正文

### P2-8: 统一文档转换层
- **42md对应**: converters/document.rs
- **质量影响**: ★★★☆☆ 架构提升，便于扩展
- **技术难度**: ★★★☆☆ 定义trait+实现
- **AI额度**: 否
- **方案**: DocumentConverter trait，所有输入→Markdown

### P2-9: 网页截图
- **42md对应**: tools/screenshot/mod.rs
- **质量影响**: ★★☆☆☆ 图示卡素材获取
- **技术难度**: ★★★★☆ 需要Chrome/headless_chrome
- **AI额度**: 否
- **方案**: headless_chrome crate或subprocess调用chrome

### P2-10: 图片OCR预处理
- **42md对应**: image/preprocess.rs + converters/image.rs
- **质量影响**: ★★★☆☆ 扫描版PDF中的图片文字
- **技术难度**: ★★★☆☆ 集成OCR引擎
- **AI额度**: 否（本地OCR如tesseract）

### P2-11: 任务队列系统
- **42md对应**: task/mod.rs + pipeline.rs + registry.rs
- **质量影响**: ★★☆☆☆ 批量处理时更稳定
- **技术难度**: ★★★☆☆ 状态机+持久化
- **AI额度**: 否
- **方案**: 简单内存队列即可，不需要持久化

---

## P3 — 长期实施（难度高或锦上添花）

### P3-1: 卡片精炼闭环（lint→refine→synthesize）
- **42md对应**: compile/card_lint.rs + card_refine.rs + synthesize.rs
- **质量影响**: ★★★★★ 最高质量的保证机制
- **技术难度**: ★★★★★ 需要多轮LLM调用
- **AI额度**: 是（lint+refine+synthesize各需LLM）
- **说明**: 这是42md的核心技术，但消耗大量LLM额度
- **建议**: 可选功能，默认关闭

### P3-2: Typst排版引擎集成
- **42md对应**: tools/md2pdf/engine.rs
- **质量影响**: ★★★★☆ 出版级PDF质量
- **技术难度**: ★★★★★ typst crate集成复杂
- **AI额度**: 否
- **说明**: 42md用typst做md2pdf，质量极高但体积大
- **替代**: 可用printpdf/genpdf做轻量版

### P3-3: CDP浏览器自动化
- **42md对应**: url/browser.rs
- **质量影响**: ★★★☆☆ 处理JS渲染页面
- **技术难度**: ★★★★★ CDP协议复杂
- **AI额度**: 否
- **说明**: 需要Chrome/Chromium，200MB+

### P3-4: URL适配器系统
- **42md对应**: url/adapters/
- **质量影响**: ★★★☆☆ 提升特定网站提取质量
- **技术难度**: ★★★☆☆ 每个适配器独立
- **AI额度**: 否
- **说明**: GitHub/维基百科/StackOverflow等专用解析

### P3-5: 整站资源下载
- **42md对应**: tools/download/mod.rs
- **质量影响**: ★★☆☆☆ 批量获取资源
- **技术难度**: ★★★☆☆ 递归爬取+去重
- **AI额度**: 否
- **说明**: 与核心卡片编译关联度低

### P3-6: 音频转录
- **42md对应**: asr/
- **质量影响**: ★★☆☆☆ 扩展输入类型
- **技术难度**: ★★★★☆ symphonia+whisper
- **AI额度**: 是（whisper需要GPU/云端）
- **说明**: 与PDF核心目标无关

### P3-7: 字幕解析
- **42md对应**: document/subtitle.rs
- **质量影响**: ★★☆☆☆ 视频字幕转卡片
- **技术难度**: ★★☆☆☆ SRT/ASS格式简单
- **AI额度**: 否
- **说明**: 与PDF核心目标无关

### P3-8: 文献交叉引用
- **42md对应**: metadata/crossref.rs
- **质量影响**: ★★★☆☆ 学术文献关联
- **技术难度**: ★★★☆☆ 需要Crossref API
- **AI额度**: 否（但需网络）
- **说明**: 学术场景有用

### P3-9: 数学公式渲染
- **42md对应**: tools/md2pdf/math/mod.rs
- **质量影响**: ★★★☆☆ 理工科文档必需
- **技术难度**: ★★★★☆ LaTeX→Typst转换
- **AI额度**: 否
- **说明**: 特定领域需求

### P3-10: 本地LLM支持（Ollama）
- **42md对应**: common/llm.rs
- **质量影响**: ★★★★☆ 完全离线，零API成本
- **技术难度**: ★★★☆☆ Ollama HTTP API兼容OpenAI格式
- **AI额度**: 否（本地推理）
- **说明**: 需本地GPU，推理速度慢但隐私最好

---

## 三、排除清单（需要AI额度或与目标无关）

| # | 功能 | 排除原因 |
|---|------|---------|
| E1 | 热词提取 (hotwords) | 需要LLM额度 |
| E2 | AI改进 (improve) | 需要LLM额度 |
| E3 | 翻译 (translate) | 需要LLM额度 |
| E4 | 音频转录 (ASR) | 需要ASR API，与PDF无关 |
| E5 | 云端同步 (result_sync) | 云端依赖，违背本地原则 |
| E6 | 认证/登录 (auth) | 商业功能，不需要 |
| E7 | 配额管理 (quota) | 商业功能，不需要 |
| E8 | 自动升级 (upgrade) | 非核心功能 |
| E9 | 远程PDF处理 (pdf/remote) | 云端依赖 |
| E10 | MOBI/iWork/RTF/OFD/WebArchive | 输入格式，与PDF核心目标弱相关 |
| E11 | 公众号HTML (md2wechat) | 输出格式，优先级低 |
| E12 | DOCX输出 (md2docx) | 输出格式，优先级低 |

---

## 四、实施路线图

### Phase 1 (2周) — P0全部
- [ ] P0-1: 卡片类型扩展 ✅ 已完成
- [ ] P0-2: 分阶段独立重试
- [ ] P0-3: 语义卡片去重
- [ ] P0-4: 空卡片/低质量过滤
- [ ] P0-5: 引用一致性检查
- [ ] P0-6: PDF质量评分增强

### Phase 2 (3周) — P1全部
- [ ] P1-1: 跨Chunk实体统一
- [ ] P1-2: 跨Chunk关系合并
- [ ] P1-3: 中文排版lint
- [ ] P1-4: PDF后处理（页眉页脚/水印）
- [ ] P1-5: 策展风格分类
- [ ] P1-6: 文档类型检测

### Phase 3 (4周) — P2高优先级
- [ ] P2-3: md2html（最简单，1天）
- [ ] P2-8: 统一文档转换层（架构基础）
- [ ] P2-4: EPUB输入
- [ ] P2-5: DOCX输入
- [ ] P2-6: HTML输入
- [ ] P2-7: URL输入（基础版）
- [ ] P2-1/2: md2pdf/md2epub

### Phase 4 (6周+) — P3可选
- [ ] P3-1: 卡片精炼闭环（lint→refine）
- [ ] P3-2: Typst排版
- [ ] P3-3: CDP浏览器
- [ ] P3-10: 本地LLM

---

## 五、关键技术决策

### 决策1: 去重算法选择
| 方案 | 优点 | 缺点 |
|------|------|------|
| 文本相似度(cosine) | 简单、快速 | 对改写后的内容不敏感 |
| SimHash | 可处理大规模 | 短文本效果差 |
| Embedding相似度 | 语义理解好 | 需要embedding模型 |
| LLM判断 | 最准确 | 消耗额度 |

**建议**: Phase 1用SimHash（纯算法），Phase 4可选LLM增强。

### 决策2: lint实现路径
| 方案 | 优点 | 缺点 |
|------|------|------|
| 调用42md lint | 零开发、质量高 | 依赖外部二进制 |
| 自己实现 | 自包含 | 规则积累慢 |
| 混合方案 | 先用42md，后自实现 | 过渡期复杂 |

**建议**: 自己实现核心规则（中西文间距、标点、引号），20+规则约500行代码。

### 决策3: PDF输出方案
| 方案 | 优点 | 缺点 |
|------|------|------|
| Typst | 质量最高 | 体积40MB+ |
| headless Chrome | 兼容性好 | 需安装Chrome |
| printpdf | 轻量 | 中文支持需配置 |
| 不实现 | 零成本 | 只能输出Markdown |

**建议**: Phase 3集成typst，前期先用Markdown输出。

---

*生成时间: 2026-05-23*
*基于42md v0.7.6二进制逆向工程分析*
