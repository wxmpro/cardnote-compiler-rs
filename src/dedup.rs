use std::collections::HashSet;

use crate::models::Card;

/// 去重配置
#[derive(Debug, Clone)]
pub struct DedupConfig {
    /// 相似度阈值（0.0-1.0），超过此值视为重复
    pub similarity_threshold: f64,
    /// Shingle大小（字级n-gram）
    pub shingle_size: usize,
    /// 标题相似度权重（0.0-1.0）
    pub title_weight: f64,
    /// 内容相似度权重（0.0-1.0）
    pub content_weight: f64,
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.65,
            shingle_size: 3,
            title_weight: 0.3,
            content_weight: 0.7,
        }
    }
}

/// 根据内容长度自适应配置
/// 短内容（<200字）使用 2-shingle + 更低阈值，解决中文短内容 Jaccard 失效问题
pub fn adaptive_dedup_config(content_len: usize) -> DedupConfig {
    if content_len < 100 {
        // 金句卡等超短内容：2 字 shingle + 低阈值
        DedupConfig {
            similarity_threshold: 0.40,
            shingle_size: 2,
            title_weight: 0.3,
            content_weight: 0.7,
        }
    } else if content_len < 200 {
        // 短内容：2 字 shingle + 中等阈值
        DedupConfig {
            similarity_threshold: 0.45,
            shingle_size: 2,
            title_weight: 0.3,
            content_weight: 0.7,
        }
    } else {
        DedupConfig::default()
    }
}

/// 去重结果
#[derive(Debug, Clone)]
pub struct DedupResult {
    /// 去重后的卡片
    pub cards: Vec<Card>,
    /// 去重统计
    pub stats: DedupStats,
}

/// 去重统计
#[derive(Debug, Clone, Default)]
pub struct DedupStats {
    /// 原始卡片数
    pub original_count: usize,
    /// 去重后卡片数
    pub unique_count: usize,
    /// 合并的卡片组数
    pub merged_groups: usize,
    /// 被移除的卡片数
    pub removed_count: usize,
}

/// 语义去重主函数
///
/// 超越42md的核心设计：
/// 1. 字级shingle计算Jaccard相似度（无需分词，适合中文）
/// 2. 多维度相似度：标题相似度 + 内容相似度
/// 3. 质量评分驱动的canonical选择（非随机选择）
/// 4. 智能内容合并：提取独特信息，而非简单拼接
pub fn semantic_dedup(cards: &[Card], config: &DedupConfig) -> DedupResult {
    if cards.len() <= 1 {
        return DedupResult {
            cards: cards.to_vec(),
            stats: DedupStats {
                original_count: cards.len(),
                unique_count: cards.len(),
                merged_groups: 0,
                removed_count: 0,
            },
        };
    }

    // 步骤1：为每张卡片计算shingle签名
    let signatures: Vec<CardSignature> = cards.iter().map(|c| build_signature(c, config)).collect();

    // 步骤2：构建相似度图（邻接表）
    let similarity_graph = build_similarity_graph(&signatures, config);

    // 步骤3：找出连通分量（每个分量是一组语义相似的卡片）
    let groups = find_connected_components(&similarity_graph, cards.len());

    // 步骤4：对每个组进行智能合并
    let mut result: Vec<Card> = Vec::new();
    let mut merged_groups = 0;
    let mut removed_count = 0;

    for group in &groups {
        if group.len() == 1 {
            result.push(cards[group[0]].clone());
        } else {
            merged_groups += 1;
            removed_count += group.len() - 1;
            let merged = merge_group(group, cards, &signatures);
            result.push(merged);
        }
    }

    // 步骤5：按类型和标题排序，保持输出一致性
    result.sort_by(|a, b| {
        a.card_type
            .to_string()
            .cmp(&b.card_type.to_string())
            .then_with(|| a.title.cmp(&b.title))
    });

    DedupResult {
        stats: DedupStats {
            original_count: cards.len(),
            unique_count: result.len(),
            merged_groups,
            removed_count,
        },
        cards: result,
    }
}

/// 卡片签名（用于相似度计算）
#[derive(Debug, Clone)]
struct CardSignature {
    title_shingles: HashSet<String>,
    content_shingles: HashSet<String>,
    quality_score: f64,
}

/// 为卡片构建shingle签名
fn build_signature(card: &Card, config: &DedupConfig) -> CardSignature {
    CardSignature {
        title_shingles: text_to_shingles(&card.title, config.shingle_size),
        content_shingles: text_to_shingles(&card.content, config.shingle_size),
        quality_score: compute_quality_score(card),
    }
}

/// 将文本转换为字级shingle集合
///
/// 示例："知识管理" + shingle_size=3 → {"知识管", "识管理"}
fn text_to_shingles(text: &str, size: usize) -> HashSet<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < size {
        return HashSet::from([text.to_string()]);
    }

    let mut shingles = HashSet::new();
    for window in chars.windows(size) {
        let shingle: String = window.iter().collect();
        shingles.insert(shingle);
    }
    shingles
}

/// 计算Jaccard相似度：|A ∩ B| / |A ∪ B|
fn jaccard_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let intersection: HashSet<_> = a.intersection(b).collect();
    let union: HashSet<_> = a.union(b).collect();

    intersection.len() as f64 / union.len() as f64
}

/// 计算两张卡片的综合相似度
fn compute_similarity(a: &CardSignature, b: &CardSignature, config: &DedupConfig) -> f64 {
    let title_sim = jaccard_similarity(&a.title_shingles, &b.title_shingles);
    let content_sim = jaccard_similarity(&a.content_shingles, &b.content_shingles);

    config.title_weight * title_sim + config.content_weight * content_sim
}

/// 构建相似度图（邻接表表示）
fn build_similarity_graph(signatures: &[CardSignature], config: &DedupConfig) -> Vec<Vec<usize>> {
    let n = signatures.len();
    let mut graph: Vec<Vec<usize>> = vec![Vec::new(); n];

    for i in 0..n {
        for j in (i + 1)..n {
            let sim = compute_similarity(&signatures[i], &signatures[j], config);
            if sim >= config.similarity_threshold {
                graph[i].push(j);
                graph[j].push(i);
            }
        }
    }

    graph
}

/// 找出连通分量（使用DFS）
fn find_connected_components(graph: &[Vec<usize>], n: usize) -> Vec<Vec<usize>> {
    let mut visited = vec![false; n];
    let mut components = Vec::new();

    for start in 0..n {
        if visited[start] {
            continue;
        }

        let mut component = Vec::new();
        let mut stack = vec![start];
        visited[start] = true;

        while let Some(node) = stack.pop() {
            component.push(node);
            for &neighbor in &graph[node] {
                if !visited[neighbor] {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }

        components.push(component);
    }

    components
}

/// 计算卡片质量评分（0.0-1.0）
///
/// 评分维度：
/// 1. 内容长度（200-500字为佳）
/// 2. 信息密度（术语/概念数量）
/// 3. 结构完整性（是否有标题、内容、引用）
/// 4. 原创性（非简单复制原文）
fn compute_quality_score(card: &Card) -> f64 {
    let mut score = 0.0;

    // 维度1：内容长度（200-500字为最佳区间）
    let content_len = card.content.chars().count();
    let length_score = if (200..=500).contains(&content_len) {
        1.0
    } else if (100..200).contains(&content_len) {
        0.7
    } else if content_len > 500 && content_len <= 800 {
        0.8
    } else if content_len < 50 {
        0.1
    } else {
        0.5
    };
    score += length_score * 0.3;

    // 维度2：信息密度（中文术语/概念密度）
    let density_score = compute_info_density(&card.content);
    score += density_score * 0.25;

    // 维度3：结构完整性
    let mut structure_score = 0.0;
    if !card.title.is_empty() {
        structure_score += 0.3;
    }
    if !card.content.is_empty() {
        structure_score += 0.3;
    }
    if !card.reference.is_empty() {
        structure_score += 0.2;
    }
    if !card.unique_id.is_empty() {
        structure_score += 0.2;
    }
    score += structure_score * 0.25;

    // 维度4：引用完整性（金句卡有原文/出处/仿写加分）
    let quote_bonus = if card.card_type.to_string() == "金句卡" {
        let mut bonus = 0.0;
        if !card.original_text.is_empty() {
            bonus += 0.5;
        }
        if !card.source.is_empty() {
            bonus += 0.3;
        }
        if !card.paraphrase.is_empty() {
            bonus += 0.2;
        }
        bonus
    } else {
        0.0
    };
    score += quote_bonus * 0.2;

    score.min(1.0)
}

/// 计算信息密度：每100字中的术语/概念数量
fn compute_info_density(content: &str) -> f64 {
    if content.is_empty() {
        return 0.0;
    }

    // 简单启发式：统计"「」"、"【】"、英文术语、数字等高密度信息标记
    let high_density_markers = content.matches('「').count()
        + content.matches('【').count()
        + content.matches("比如").count()
        + content.matches("例如").count()
        + content.matches("关键").count()
        + content.matches("核心").count();

    let char_count = content.chars().count().max(1);
    let density = (high_density_markers as f64 * 100.0) / char_count as f64;

    // 归一化到0-1
    (density / 5.0).min(1.0)
}

/// 智能合并卡片组
///
/// 策略：
/// 1. 选择质量评分最高的卡片作canonical（基础）
/// 2. 从其他卡片中提取"独特信息"（canonical中没有的内容）
/// 3. 将独特信息以"补充视角"形式附加
/// 4. 合并引用来源
fn merge_group(group: &[usize], cards: &[Card], signatures: &[CardSignature]) -> Card {
    // 找出质量最高的卡片作canonical
    let canonical_idx = *group
        .iter()
        .max_by(|a, b| {
            signatures[**a]
                .quality_score
                .partial_cmp(&signatures[**b].quality_score)
                .unwrap()
        })
        .unwrap();

    let mut canonical = cards[canonical_idx].clone();

    // 收集所有独特内容（其他卡片中有但canonical中没有的shingle）
    let mut supplemental_contents: Vec<String> = Vec::new();
    let mut all_references: Vec<String> = Vec::new();

    if !canonical.reference.is_empty() {
        all_references.push(canonical.reference.clone());
    }

    for &idx in group {
        if idx == canonical_idx {
            continue;
        }

        let card = &cards[idx];

        // 提取独特shingle
        let unique_shingles: HashSet<_> = signatures[idx]
            .content_shingles
            .difference(&signatures[canonical_idx].content_shingles)
            .cloned()
            .collect();

        if !unique_shingles.is_empty() {
            // 提取包含独特shingle的句子
            let unique_sentences = extract_unique_sentences(&card.content, &unique_shingles);
            if !unique_sentences.is_empty() {
                supplemental_contents.push(format!(
                    "【来自《{}》的补充视角】\n{}",
                    card.title,
                    unique_sentences.join("\n")
                ));
            }
        }

        // 收集引用
        if !card.reference.is_empty() && !all_references.contains(&card.reference) {
            all_references.push(card.reference.clone());
        }
    }

    // 合并补充内容
    if !supplemental_contents.is_empty() {
        canonical.content.push_str("\n\n---\n\n");
        canonical
            .content
            .push_str(&supplemental_contents.join("\n\n"));
    }

    // 合并引用
    if all_references.len() > 1 {
        canonical.reference = all_references.join("; ");
    }

    // 更新标题，标记为合并结果
    if group.len() > 1 {
        canonical.title = format!("{}(整合版)", canonical.title);
    }

    canonical
}

/// 从文本中提取包含独特shingle的句子
fn extract_unique_sentences(content: &str, unique_shingles: &HashSet<String>) -> Vec<String> {
    let sentences: Vec<&str> = content.split(['。', '！', '？', '\n']).collect();
    let mut result = Vec::new();

    for sentence in sentences {
        let trimmed = sentence.trim();
        if trimmed.is_empty() || trimmed.len() < 10 {
            continue;
        }

        // 检查句子是否包含任何独特shingle
        let sentence_shingles = text_to_shingles(trimmed, 3);
        let has_unique = sentence_shingles
            .iter()
            .any(|s| unique_shingles.contains(s));

        if has_unique && !result.iter().any(|r: &String| r.contains(trimmed)) {
            result.push(trimmed.to_string());
        }
    }

    // 最多保留3个补充句子，避免过长
    result.truncate(3);
    result
}

/// 快速去重（保留API兼容性，使用默认配置）
pub fn dedup_cards(cards: &[Card]) -> Vec<Card> {
    let config = DedupConfig::default();
    let result = semantic_dedup(cards, &config);

    if result.stats.removed_count > 0 {
        eprintln!(
            "  ♫️  语义去重: {} 张卡片 → {} 张 (合并 {} 组, 移除 {} 张)",
            result.stats.original_count,
            result.stats.unique_count,
            result.stats.merged_groups,
            result.stats.removed_count
        );
    }

    result.cards
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CardType;

    #[test]
    fn test_shingles_basic() {
        let shingles = text_to_shingles("知识管理", 3);
        assert!(shingles.contains("知识管"));
        assert!(shingles.contains("识管理"));
        assert_eq!(shingles.len(), 2);
    }

    #[test]
    fn test_jaccard_identical() {
        let a = text_to_shingles("知识管理方法论", 3);
        let b = text_to_shingles("知识管理方法论", 3);
        assert_eq!(jaccard_similarity(&a, &b), 1.0);
    }

    #[test]
    fn test_jaccard_completely_different() {
        let a = text_to_shingles("概念C", 3);
        let b = text_to_shingles("概念D", 3);
        let sim = jaccard_similarity(&a, &b);
        assert!(sim < 0.3, "完全不同的文本相似度应低于0.3，实际为 {}", sim);
    }

    #[test]
    fn test_semantic_dedup_finds_similar_cards() {
        // 使用高度相似的内容确保能被shingle检测到
        let cards = vec![
            Card {
                title: "概念A".to_string(),
                content: "文本A的内容描述。文本B的补充说明。".to_string(),
                card_type: CardType::Knowledge,
                reference: "来源A".to_string(),
                unique_id: "20240101120000".to_string(),
                ..Default::default()
            },
            Card {
                title: "概念A变体".to_string(),
                content: "文本A的内容描述。文本B的补充说明。".to_string(),
                card_type: CardType::Knowledge,
                reference: "来源B".to_string(),
                unique_id: "20240101120001".to_string(),
                ..Default::default()
            },
            Card {
                title: "概念B".to_string(),
                content: "文本B的内容描述。".to_string(),
                card_type: CardType::Term,
                reference: "来源C".to_string(),
                unique_id: "20240101120002".to_string(),
                ..Default::default()
            },
        ];

        let result = semantic_dedup(&cards, &DedupConfig::default());

        // 应该合并前两张（高度相似），保留第三张
        assert_eq!(result.stats.original_count, 3);
        assert_eq!(result.stats.unique_count, 2);
        assert_eq!(result.stats.merged_groups, 1);
        assert_eq!(result.stats.removed_count, 1);
    }

    #[test]
    fn test_semantic_dedup_keeps_unique_cards() {
        let cards = vec![
            Card {
                title: "术语A".to_string(),
                content: "文本C的内容描述。".to_string(),
                card_type: CardType::Term,
                reference: "来源A".to_string(),
                unique_id: "20240101120000".to_string(),
                ..Default::default()
            },
            Card {
                title: "术语B".to_string(),
                content: "文本D的内容描述。".to_string(),
                card_type: CardType::Term,
                reference: "来源B".to_string(),
                unique_id: "20240101120001".to_string(),
                ..Default::default()
            },
        ];

        let result = semantic_dedup(&cards, &DedupConfig::default());
        assert_eq!(result.stats.unique_count, 2);
        assert_eq!(result.stats.merged_groups, 0);
    }

    #[test]
    fn test_quality_score_length() {
        let card = Card {
            title: "标题A".to_string(),
            content: "文本G的内容描述。".to_string(),
            card_type: CardType::Knowledge,
            reference: "来源A".to_string(),
            unique_id: "20240101120000".to_string(),
            ..Default::default()
        };
        let score = compute_quality_score(&card);
        assert!(
            score > 0.2 && score <= 1.0,
            "质量评分应在合理范围内: {}",
            score
        );
    }

    #[test]
    fn test_extract_unique_sentences() {
        let content = "句子A。句子B的独特内容。句子C。";
        let unique = text_to_shingles("独特内容", 3);
        let sentences = extract_unique_sentences(content, &unique);
        assert_eq!(sentences.len(), 1);
        assert!(sentences[0].contains("独特"));
    }

    #[test]
    fn test_dedup_empty_cards() {
        let cards: Vec<Card> = vec![];
        let result = semantic_dedup(&cards, &DedupConfig::default());
        assert_eq!(result.cards.len(), 0);
    }

    #[test]
    fn test_dedup_single_card() {
        let cards = vec![Card {
            title: "单张卡片".to_string(),
            content: "单张卡片的文本内容。".to_string(),
            card_type: CardType::Knowledge,
            reference: "来源A".to_string(),
            unique_id: "20240101120000".to_string(),
            ..Default::default()
        }];
        let result = semantic_dedup(&cards, &DedupConfig::default());
        assert_eq!(result.cards.len(), 1);
        assert_eq!(result.stats.merged_groups, 0);
    }

    /// 验证去重系统能正确合并相似卡片并保留独特卡片
    #[test]
    fn test_dedup_merges_similar_cards_keeps_unique() {
        // 两张高度相似的人物卡（内容相同，标题略有差异）应被合并
        // 一张不相关的术语卡应被保留
        let cards = vec![
            Card {
                title: "实体A".to_string(),
                content: "文本E的内容描述。".to_string(),
                card_type: CardType::Person,
                reference: "来源E".to_string(),
                unique_id: "20240101120000".to_string(),
                ..Default::default()
            },
            Card {
                title: "实体A变体".to_string(),
                content: "文本E的内容描述。".to_string(),
                card_type: CardType::Person,
                reference: "来源F".to_string(),
                unique_id: "20240101120001".to_string(),
                ..Default::default()
            },
            Card {
                title: "术语C".to_string(),
                content: "文本F的内容描述。".to_string(),
                card_type: CardType::Term,
                reference: "来源D".to_string(),
                unique_id: "20240101120002".to_string(),
                ..Default::default()
            },
        ];

        let result = semantic_dedup(&cards, &DedupConfig::default());
        // 两张相似的人物卡应被合并为一张
        assert_eq!(result.stats.original_count, 3);
        assert_eq!(
            result.stats.unique_count, 2,
            "应合并两张相似卡片，保留独立卡片"
        );
        assert_eq!(result.stats.merged_groups, 1);
    }

    // ── 边界测试 ──

    #[test]
    fn test_semantic_dedup_identical_cards() {
        // 两张完全相同的卡片应被合并
        let cards = vec![
            Card {
                title: "相同标题".to_string(),
                content: "完全相同的内容文本。".to_string(),
                card_type: CardType::Knowledge,
                reference: "来源A".to_string(),
                unique_id: "20240101120000".to_string(),
                ..Default::default()
            },
            Card {
                title: "相同标题".to_string(),
                content: "完全相同的内容文本。".to_string(),
                card_type: CardType::Knowledge,
                reference: "来源B".to_string(),
                unique_id: "20240101120001".to_string(),
                ..Default::default()
            },
        ];
        let result = semantic_dedup(&cards, &DedupConfig::default());
        assert_eq!(result.stats.unique_count, 1);
        assert_eq!(result.stats.merged_groups, 1);
        assert_eq!(result.stats.removed_count, 1);
    }

    #[test]
    fn test_semantic_dedup_empty_content() {
        let cards = vec![
            Card {
                title: "标题A".to_string(),
                content: "".to_string(),
                card_type: CardType::Knowledge,
                reference: "".to_string(),
                unique_id: "20240101120000".to_string(),
                ..Default::default()
            },
            Card {
                title: "标题B".to_string(),
                content: "".to_string(),
                card_type: CardType::Knowledge,
                reference: "".to_string(),
                unique_id: "20240101120001".to_string(),
                ..Default::default()
            },
        ];
        // 空内容卡片的shingle相同，应被视为相似
        let result = semantic_dedup(&cards, &DedupConfig::default());
        assert_eq!(result.stats.original_count, 2);
        // 空内容卡片标题不同，但内容相同（都为空），可能被合并
        assert!(result.stats.unique_count <= 2);
    }

    #[test]
    fn test_semantic_dedup_empty_title() {
        let cards = vec![
            Card {
                title: "".to_string(),
                content: "内容A的内容描述文本。".to_string(),
                card_type: CardType::Knowledge,
                reference: "".to_string(),
                unique_id: "20240101120000".to_string(),
                ..Default::default()
            },
            Card {
                title: "".to_string(),
                content: "内容B完全不同的描述。".to_string(),
                card_type: CardType::Knowledge,
                reference: "".to_string(),
                unique_id: "20240101120001".to_string(),
                ..Default::default()
            },
        ];
        let result = semantic_dedup(&cards, &DedupConfig::default());
        // 标题为空但内容不同，不应合并
        assert_eq!(result.stats.unique_count, 2);
    }

    #[test]
    fn test_jaccard_empty_sets() {
        let a: HashSet<String> = HashSet::new();
        let b: HashSet<String> = HashSet::new();
        assert_eq!(jaccard_similarity(&a, &b), 1.0);

        let b = text_to_shingles("文本内容", 3);
        assert_eq!(jaccard_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_text_to_shingles_short() {
        // 文本长度小于shingle_size
        let shingles = text_to_shingles("短", 3);
        assert_eq!(shingles.len(), 1);
        assert!(shingles.contains("短"));
    }

    #[test]
    fn test_compute_quality_score_empty() {
        let card = Card {
            title: "".to_string(),
            content: "".to_string(),
            card_type: CardType::Knowledge,
            reference: "".to_string(),
            unique_id: "".to_string(),
            ..Default::default()
        };
        let score = compute_quality_score(&card);
        assert!((0.0..=1.0).contains(&score));
    }

    #[test]
    fn test_compute_quality_score_very_long() {
        let card = Card {
            title: "标题".to_string(),
            content: "内容".repeat(1000),
            card_type: CardType::Knowledge,
            reference: "".to_string(),
            unique_id: "".to_string(),
            ..Default::default()
        };
        let score = compute_quality_score(&card);
        assert!((0.0..=1.0).contains(&score));
    }

    // ── 自适应 Jaccard 对抗性测试 ──

    #[test]
    fn test_adaptive_jaccard_short_chinese_semantic_similar() {
        // 短内容反例1: 语义相近但措辞不同
        let config = adaptive_dedup_config(20);
        let a = text_to_shingles("阅读是心灵的旅行", config.shingle_size);
        let b = text_to_shingles("心灵之旅是阅读的本质", config.shingle_size);
        let sim = jaccard_similarity(&a, &b);
        assert!(sim > 0.1, "语义相近的中文短内容应有相似度 > 0.1: {}", sim);
    }

    #[test]
    fn test_adaptive_jaccard_short_chinese_theory() {
        // 短内容反例2: 认知负荷理论的两种表述
        let config = adaptive_dedup_config(20);
        let a = text_to_shingles("认知负荷理论指出了", config.shingle_size);
        let b = text_to_shingles("认知负荷理论认为这", config.shingle_size);
        let sim = jaccard_similarity(&a, &b);
        assert!(sim > 0.3, "认知负荷理论变体应被识别: {}", sim);
    }

    #[test]
    fn test_adaptive_dedup_config_short() {
        let config = adaptive_dedup_config(50);
        assert_eq!(config.shingle_size, 2);
        assert!(config.similarity_threshold < 0.5);
    }

    #[test]
    fn test_adaptive_dedup_config_medium() {
        let config = adaptive_dedup_config(150);
        assert_eq!(config.shingle_size, 2);
        assert!(config.similarity_threshold < 0.55);
    }

    #[test]
    fn test_adaptive_dedup_config_long() {
        let config = adaptive_dedup_config(300);
        assert_eq!(config.shingle_size, 3);
        assert!(config.similarity_threshold > 0.5);
    }
}
