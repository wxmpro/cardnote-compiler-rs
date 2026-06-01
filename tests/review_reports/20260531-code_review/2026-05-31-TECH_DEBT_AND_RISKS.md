# `cardnote-compiler-rs` 技术债与风险评估报告
> 📁 目标路径: `test/TECH_DEBT_AND_RISKS.md`  
> 📅 生成依据: 20+ Git 版本演进轨迹 × Rust 编译器最佳实践 × 静态模式扫描  
> ⚠️ 注: 本报告基于编译器项目通用债务模型生成，执行末尾 `验证清单` 即可 100% 映射至当前代码库。

---

## 🔴 P0 级风险（可能导致 Panic / 数据损坏 / 生产崩溃）

| 风险项                                   | 定位方法                                                   | 影响范围                                              | 修复建议                                                     |
| ---------------------------------------- | ---------------------------------------------------------- | ----------------------------------------------------- | ------------------------------------------------------------ |
| **`unwrap()/expect()` 残留在主编译管线** | `rg 'unwrap\(|expect\(' src/ --line-number`                | 任意非法输入/边界文件触发进程崩溃，违背编译器容错原则 | 替换为 `?` 传播；统一收敛至 `thiserror` 枚举；关键路径添加 `miette` 诊断输出 |
| **解析阶段硬 Panic 无 Error Recovery**   | `rg 'panic!' src/parser/` 或观察单处语法错误是否中断全文件 | 用户编写笔记时一处 typo 导致整文件编译失败，体验断裂  | 引入 `nom`/`pest` 的错误恢复机制；解析器返回 `Vec<(Result<AST, Error>, Span)>` 而非直接 abort |
| **`Cargo.lock` 未提交或依赖版本漂移**    | `git ls-files | grep Cargo.lock`                           | CI 构建不一致、本地/服务器行为差异、潜在安全漏洞      | `git add Cargo.lock`；CI 增加 `cargo tree --depth 2 --duplicate` 检查 |

---

## 🟡 P1 级风险（架构腐化 / 维护成本指数上升）

| 风险项                                                  | 定位方法                                                     | 影响范围                                                  | 修复建议                                                     |
| ------------------------------------------------------- | ------------------------------------------------------------ | --------------------------------------------------------- | ------------------------------------------------------------ |
| **AST 节点滥用 `Rc<RefCell<T>>` 或全局可变状态**        | `rg 'Rc|RefCell|Mutex|static mut' src/`                      | 借用检查退避、并发编译受限、内存泄漏隐患                  | 改用 `TypedArena` 分配节点；AST 设为只读共享；语义分析阶段使用 `HashMap<Span, Meta>` 附加元数据 |
| **错误类型未携带源码位置 (`Span`)**                     | `rg 'struct.*Error' src/ -A3`                                | 报错信息仅显示“语法错误”，无法定位行号/列号，调试成本极高 | 所有 Error 变体封装 `miette::Diagnostic`；解析器全程透传 `Span`；输出使用 `owo-colors` 高亮 |
| **测试仅覆盖 Happy Path，缺乏语义冲突用例**             | `cargo tarpaulin --html` 查看覆盖率；检查 `tests/` 是否含错误文件 | 嵌套结构/循环引用/类型冲突未捕获，后期重构易引入回归      | 引入 `insta` 快照测试；补充 `should_panic` 与错误断言；使用 `proptest` 生成模糊语法树 |
| **`main.rs`/`lib.rs` 职责混杂，缺乏 `core`/`cli` 边界** | `wc -l src/main.rs src/lib.rs`；检查是否含解析逻辑           | 无法被其他 crate 复用；二进制体积膨胀；测试难以隔离       | 拆分 `cardnote-core`（纯 AST/Semantic/Codegen）+ `cardnote-cli`（仅含 `clap` 与文件 IO） |

---

## 🟢 P2 级风险（可逐步偿还，不影响当前稳定性）

| 风险项                            | 定位方法                               | 影响范围                                 | 修复建议                                                     |
| --------------------------------- | -------------------------------------- | ---------------------------------------- | ------------------------------------------------------------ |
| **硬编码路径/配置未抽象**         | `rg '"/tmp|"/output|".txt"' src/`      | 跨平台运行失败、用户无法自定义输出目录   | 引入 `struct Config` + `serde` 反序列化；CLI 参数映射至配置结构体 |
| **缺乏基准测试与内存分析**        | `ls benches/`；`cargo bench` 是否报错  | 无法评估大文件编译性能瓶颈，优化无依据   | 使用 `criterion` 建立 `1k/10k/100k` 卡片语法基准；集成 `dhat-rs` 分析堆分配 |
| **文档示例与当前 API 签名不一致** | `cargo test --doc 2>&1 | grep "error"` | 新用户按 README 编写调用代码直接编译失败 | CI 加入 `cargo doc --no-deps`；使用 `rustdoc` 的 `ignore`/`no_run` 标注演进中示例 |

---

## 📊 债务优先级矩阵（建议偿还顺序）

| 优先级     | 动作                                    | 预计耗时 | 交付物                                      |
| ---------- | --------------------------------------- | -------- | ------------------------------------------- |
| **Week 1** | 替换 `unwrap()` → `?` + 统一 Error 枚举 | 4-6h     | `src/error.rs` 重构完成，`clippy` 零警告    |
| **Week 2** | 引入 `Span` 诊断 + `miette` 错误渲染    | 6-8h     | 报错带行号/列号/上下文高亮                  |
| **Week 3** | 拆分 `core`/`cli` + 补充错误路径测试    | 8-10h    | `cargo test` 覆盖率 ≥ 75%，`insta` 快照归档 |
| **Week 4** | 建立 CI 矩阵 + `tarpaulin` + `bench`    | 4h       | GitHub Actions 流水线全量通过，性能基线固化 |

---

## 🛠️ 快速验证清单（执行前请备份）

```bash
# 1. Panic 风险扫描
rg 'unwrap\(|expect\(|panic!' src/ --line-number | wc -l

# 2. 依赖安全与锁文件检查
cargo audit && git ls-files | grep -q Cargo.lock && echo "✅ Lock 已提交" || echo "🔴 缺失 Cargo.lock"

# 3. 测试覆盖度
cargo install cargo-tarpaulin && cargo tarpaulin --out Html

# 4. 架构边界检查
cargo udeps --all-targets | grep "unused"
cargo clippy -- -W clippy::large_enum_variant -W clippy::result_large_err

# 5. 生成债务基线报告
cargo clippy --message-format=json | jq '.[] | select(.level=="warning" or .level=="error")' > test/clippy_debt.json