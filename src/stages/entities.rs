use serde_json::Value;

use crate::config::doc_limits_for;
use crate::error::Result;
use crate::models::{Entity, LlmMessage};

/// 提取实体
pub async fn extract_entities(
    document: &str,
    _call_llm: impl ChatFn,
    call_llm_json: impl ChatJsonFn,
    load_prompt: &(dyn Fn(&str) -> Result<String> + Send + Sync),
) -> Result<Vec<Entity>> {
    let prompt_template = load_prompt("entity_extraction")?;
    let prompt = prompt_template.replace("{document}", document);

    let system = "你是一位实体识别专家。请严格按 JSON 格式输出。".to_string();

    let response = match call_llm_json
        .call_json(
            vec![
                LlmMessage {
                    role: "system".to_string(),
                    content: system,
                },
                LlmMessage {
                    role: "user".to_string(),
                    content: prompt,
                },
            ],
            Some(doc_limits_for(document.chars().count()).entity_output as u32),
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("    ⚠ 实体识别 LLM 调用失败，返回空列表: {}", e);
            return Ok(Vec::new());
        }
    };

    match parse_entities(&response) {
        Ok(entities) => Ok(entities),
        Err(e) => {
            eprintln!("    ⚠ 实体解析失败，返回空列表: {}", e);
            Ok(Vec::new())
        }
    }
}

/// 实体去重（基础精确匹配）
pub fn dedup_entities(entities: &[Entity]) -> Vec<Entity> {
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for entity in entities {
        let key = format!("{}:{}", entity.name.to_lowercase(), entity.entity_type);
        if seen.insert(key) {
            unique.push(entity.clone());
        }
    }
    unique
}

/// 跨Chunk实体统一（智能合并）
///
/// 核心设计：
/// 1. 名称规范化（去除括号注释、统一空格）
/// 2. 三级匹配：精确匹配 → 包含匹配 → 编辑距离匹配
/// 3. 智能合并：选择最佳名称 + 合并上下文
/// 4. 统计输出：合并了多少组、消除了多少重复
///
/// 返回：统一后的实体列表、统计信息、名称映射（原始名→统一名）
pub fn unify_entities(
    entities: &[Entity],
) -> (
    Vec<Entity>,
    EntityUnifyStats,
    std::collections::HashMap<String, String>,
) {
    if entities.len() <= 1 {
        let mut map = std::collections::HashMap::new();
        for e in entities {
            map.insert(e.name.clone(), e.name.clone());
        }
        return (
            entities.to_vec(),
            EntityUnifyStats {
                original_count: entities.len(),
                unified_count: entities.len(),
                merged_groups: 0,
                eliminated_duplicates: 0,
            },
            map,
        );
    }

    // 按类型分组，只在同类型内统一
    let mut by_type: std::collections::HashMap<String, Vec<&Entity>> =
        std::collections::HashMap::new();
    for e in entities {
        by_type.entry(e.entity_type.clone()).or_default().push(e);
    }

    let mut result = Vec::new();
    let mut merged_groups = 0;
    let mut eliminated_duplicates = 0;
    let mut name_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for (_etype, group) in by_type {
        let (unified, group_merged, group_eliminated, group_map) = unify_entity_group(&group);
        result.extend(unified);
        merged_groups += group_merged;
        eliminated_duplicates += group_eliminated;
        name_map.extend(group_map);
    }

    // 按名称排序，保持输出一致性
    result.sort_by(|a, b| a.name.cmp(&b.name));

    let stats = EntityUnifyStats {
        original_count: entities.len(),
        unified_count: result.len(),
        merged_groups,
        eliminated_duplicates,
    };

    (result, stats, name_map)
}

/// 实体统一统计
#[derive(Debug, Clone, Default)]
pub struct EntityUnifyStats {
    pub original_count: usize,
    pub unified_count: usize,
    pub merged_groups: usize,
    pub eliminated_duplicates: usize,
}

/// 统一同类型实体组
///
/// 返回：统一后的实体、合并组数、消除的重复数、名称映射（原始名→统一名）
fn unify_entity_group(
    entities: &[&Entity],
) -> (
    Vec<Entity>,
    usize,
    usize,
    std::collections::HashMap<String, String>,
) {
    if entities.len() <= 1 {
        let mut map = std::collections::HashMap::new();
        for e in entities {
            map.insert(e.name.clone(), e.name.clone());
        }
        return (entities.iter().map(|e| (*e).clone()).collect(), 0, 0, map);
    }

    let mut groups: Vec<Vec<&Entity>> = Vec::new();
    let mut assigned = vec![false; entities.len()];

    for i in 0..entities.len() {
        if assigned[i] {
            continue;
        }

        let mut group = vec![entities[i]];
        assigned[i] = true;

        for j in (i + 1)..entities.len() {
            if assigned[j] {
                continue;
            }
            if is_same_entity(entities[i], entities[j]) {
                group.push(entities[j]);
                assigned[j] = true;
            }
        }

        groups.push(group);
    }

    let mut result = Vec::new();
    let mut merged_groups = 0;
    let mut eliminated = 0;
    let mut name_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for group in groups {
        if group.len() == 1 {
            let entity = group[0].clone();
            name_map.insert(entity.name.clone(), entity.name.clone());
            result.push(entity);
        } else {
            merged_groups += 1;
            eliminated += group.len() - 1;
            let merged = merge_entity_group(&group);
            // 记录映射：组内所有原始名称都映射到统一名称
            for e in &group {
                name_map.insert(e.name.clone(), merged.name.clone());
            }
            result.push(merged);
        }
    }

    (result, merged_groups, eliminated, name_map)
}

/// 判断两个实体是否为同一实体
///
/// 三级匹配策略：
/// 1. 精确匹配（规范化后）
/// 2. 包含匹配（短名被长名包含）
/// 3. 编辑距离相似度（≥0.85）
fn is_same_entity(a: &Entity, b: &Entity) -> bool {
    let na = normalize_name(&a.name);
    let nb = normalize_name(&b.name);

    if na == nb {
        return true;
    }

    // 包含匹配：如 "人物C" 被 "人物B·人物C" 包含
    if na.len() >= 4 && nb.len() >= 4 && (na.contains(&nb) || nb.contains(&na)) {
        return true;
    }

    // 英文名匹配：如 "丹尼尔·卡尼曼（Daniel Kahneman）" 和 "Kahneman"
    let en_a = extract_english_name(&a.name);
    let en_b = extract_english_name(&b.name);
    if let (Some(ea), Some(eb)) = (&en_a, &en_b) {
        // 全名匹配或姓氏匹配
        if ea == eb || ea.contains(eb) || eb.contains(ea) {
            return true;
        }
        // 姓氏匹配（取最后一个词）
        let surname_a = ea.split_whitespace().last().unwrap_or(ea);
        let surname_b = eb.split_whitespace().last().unwrap_or(eb);
        if surname_a == surname_b && surname_a.len() >= 3 {
            return true;
        }
    }

    // 编辑距离匹配（适用于拼写变体）
    let sim = name_similarity(&na, &nb);
    sim >= 0.85
}

/// 从名称中提取括号内的英文名
/// "丹尼尔·卡尼曼（Daniel Kahneman）" → "Daniel Kahneman"
fn extract_english_name(name: &str) -> Option<String> {
    // 匹配中文括号（）
    if let Some(start) = name.find('（') {
        if let Some(end) = name[start..].find('）') {
            let en = name[start + 1..start + end].trim();
            if !en.is_empty() && en.chars().any(|c| c.is_ascii_alphabetic()) {
                return Some(en.to_lowercase());
            }
        }
    }
    // 匹配英文括号 ()
    if let Some(start) = name.find('(') {
        if let Some(end) = name[start..].find(')') {
            let en = name[start + 1..start + end].trim();
            if !en.is_empty() && en.chars().any(|c| c.is_ascii_alphabetic()) {
                return Some(en.to_lowercase());
            }
        }
    }
    None
}

/// 名称规范化
///
/// 1. 去除括号及其内容（如 "人物A（职称）" → "人物A"）
/// 2. 统一中英文标点
/// 3. 去除多余空格
/// 4. 转小写（用于比较）
fn normalize_name(name: &str) -> String {
    let mut result = name.to_lowercase();

    // 去除括号内容
    result = remove_bracket_content(&result, '(', ')');
    result = remove_bracket_content(&result, '（', '）');
    result = remove_bracket_content(&result, '[', ']');
    result = remove_bracket_content(&result, '【', '】');

    // 统一分隔符
    result = result.replace('·', "").replace("・", "").replace('.', "");

    // 去除多余空格
    result = result.split_whitespace().collect::<Vec<_>>().join(" ");

    result
}

/// 去除括号及其内容
fn remove_bracket_content(s: &str, open: char, close: char) -> String {
    let mut result = String::with_capacity(s.len());
    let mut depth = 0;
    for c in s.chars() {
        if c == open {
            depth += 1;
            continue;
        }
        if c == close {
            if depth > 0 {
                depth -= 1;
            }
            continue;
        }
        if depth == 0 {
            result.push(c);
        }
    }
    result
}

/// 计算两个名称的编辑距离相似度（0.0-1.0）
fn name_similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let dist = levenshtein_distance(a, b);
    let max_len = a.chars().count().max(b.chars().count());
    if max_len == 0 {
        return 1.0;
    }
    1.0 - (dist as f64 / max_len as f64)
}

/// Levenshtein 编辑距离
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut prev = vec![0; n + 1];
    let mut curr = vec![0; n + 1];

    #[allow(clippy::needless_range_loop)]
    for j in 0..=n {
        prev[j] = j;
    }

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (curr[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

/// 合并实体组
///
/// 策略：
/// 1. 选择最佳名称（最长且最完整）
/// 2. 合并上下文（去重后连接）
fn merge_entity_group(group: &[&Entity]) -> Entity {
    let canonical = select_best_name(group);

    // 合并上下文
    let mut contexts: Vec<String> = Vec::new();
    for e in group {
        if !e.context.is_empty() && !contexts.contains(&e.context) {
            contexts.push(e.context.clone());
        }
    }

    let merged_context = if contexts.is_empty() {
        String::new()
    } else if contexts.len() == 1 {
        contexts[0].clone()
    } else {
        contexts.join("；")
    };

    Entity {
        name: canonical,
        entity_type: group[0].entity_type.clone(),
        context: merged_context,
    }
}

/// 选择最佳名称
///
/// 优先选择：
/// 1. 最长的名称（通常更完整）
/// 2. 包含分隔符（如"·"）的名称（通常是全名）
/// 3. 不含括号的原始名称
fn select_best_name(group: &[&Entity]) -> String {
    group
        .iter()
        .max_by(|a, b| {
            let score_a = name_quality_score(&a.name);
            let score_b = name_quality_score(&b.name);
            score_a.partial_cmp(&score_b).unwrap()
        })
        .map(|e| e.name.clone())
        .unwrap_or_default()
}

/// 名称质量评分（越高越好）
fn name_quality_score(name: &str) -> f64 {
    let mut score = name.chars().count() as f64;

    // 包含分隔符加分（全名特征）
    if name.contains('·') || name.contains("・") || name.contains('.') {
        score += 5.0;
    }

    // 不含括号加分（更干净）
    if !name.contains('(') && !name.contains('（') {
        score += 2.0;
    }

    // 英文首字母大写加分（规范名称）
    if name
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
    {
        score += 1.0;
    }

    score
}

/// 解析实体 JSON
fn parse_entities(value: &Value) -> Result<Vec<Entity>> {
    let entities_arr = crate::stages::common::extract_json_array(value, "entities")?;

    let mut entities = Vec::new();
    for item in entities_arr {
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        let entity_type = item
            .get("type")
            .or_else(|| item.get("entity_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("未知")
            .to_string();
        let context = item
            .get("context")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        entities.push(Entity {
            name,
            entity_type,
            context,
        });
    }

    Ok(entities)
}

use crate::stages::common::{ChatFn, ChatJsonFn};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedup_entities() {
        let entities = vec![
            Entity {
                name: "实体A".to_string(),
                entity_type: "人物".to_string(),
                context: "".to_string(),
            },
            Entity {
                name: "实体A".to_string(),
                entity_type: "人物".to_string(),
                context: "".to_string(),
            },
            Entity {
                name: "实体B".to_string(),
                entity_type: "人物".to_string(),
                context: "".to_string(),
            },
        ];
        let result = dedup_entities(&entities);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_unify_entities_exact_match() {
        let entities = vec![
            Entity {
                name: "概念A".to_string(),
                entity_type: "概念".to_string(),
                context: "某领域内的特定现象描述".to_string(),
            },
            Entity {
                name: "概念A".to_string(),
                entity_type: "概念".to_string(),
                context: "另一角度的现象描述".to_string(),
            },
        ];

        let (result, stats, _map) = unify_entities(&entities);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "概念A");
        assert!(result[0].context.contains("某领域内的特定现象描述"));
        assert!(result[0].context.contains("另一角度的现象描述"));
        assert_eq!(stats.merged_groups, 1);
        assert_eq!(stats.eliminated_duplicates, 1);
    }

    #[test]
    fn test_unify_entities_bracket_variant() {
        let entities = vec![
            Entity {
                name: "人物A（职称）".to_string(),
                entity_type: "人物".to_string(),
                context: "描述A".to_string(),
            },
            Entity {
                name: "人物A".to_string(),
                entity_type: "人物".to_string(),
                context: "描述B".to_string(),
            },
        ];

        let (result, stats, _map) = unify_entities(&entities);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "人物A（职称）"); // 最长名称
        assert_eq!(stats.merged_groups, 1);
    }

    #[test]
    fn test_unify_entities_containment() {
        let entities = vec![
            Entity {
                name: "人物B·人物C".to_string(),
                entity_type: "人物".to_string(),
                context: "描述C".to_string(),
            },
            Entity {
                name: "人物C".to_string(),
                entity_type: "人物".to_string(),
                context: "描述D".to_string(),
            },
        ];

        let (result, stats, _map) = unify_entities(&entities);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "人物B·人物C"); // 完整名
        assert!(result[0].context.contains("描述C"));
        assert!(result[0].context.contains("描述D"));
        assert_eq!(stats.merged_groups, 1);
    }

    #[test]
    fn test_unify_entities_type_separation() {
        // 同名但不同类型不应合并
        let entities = vec![
            Entity {
                name: "概念B".to_string(),
                entity_type: "概念".to_string(),
                context: "".to_string(),
            },
            Entity {
                name: "概念B".to_string(),
                entity_type: "技术".to_string(),
                context: "".to_string(),
            },
        ];

        let (result, stats, _map) = unify_entities(&entities);
        assert_eq!(result.len(), 2);
        assert_eq!(stats.merged_groups, 0);
    }

    #[test]
    fn test_unify_entities_keeps_unique() {
        let entities = vec![
            Entity {
                name: "概念C".to_string(),
                entity_type: "概念".to_string(),
                context: "".to_string(),
            },
            Entity {
                name: "概念D".to_string(),
                entity_type: "概念".to_string(),
                context: "".to_string(),
            },
        ];

        let (result, stats, _map) = unify_entities(&entities);
        assert_eq!(result.len(), 2);
        assert_eq!(stats.merged_groups, 0);
    }

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("abc", "abc"), 0);
        assert_eq!(levenshtein_distance("abc", "abd"), 1);
        assert_eq!(levenshtein_distance("abc", ""), 3);
        assert_eq!(levenshtein_distance("", "abc"), 3);
    }

    #[test]
    fn test_name_similarity() {
        assert_eq!(name_similarity("abc", "abc"), 1.0);
        assert!(name_similarity("abc", "abd") > 0.6);
        assert!(name_similarity("abc", "xyz") < 0.5);
    }

    #[test]
    fn test_normalize_name() {
        assert_eq!(normalize_name("人物A（职称）"), "人物a");
        assert_eq!(normalize_name("人物A [职称A]"), "人物a");
        assert_eq!(normalize_name("人物B·人物C"), "人物b人物c");
    }

    #[test]
    fn test_parse_entities_valid() {
        let json = serde_json::json!({
            "entities": [
                { "name": "实体A", "type": "人物", "context": "上下文A" },
                { "name": "实体B", "entity_type": "人物" }
            ]
        });
        let result = parse_entities(&json).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "实体A");
        assert_eq!(result[1].entity_type, "人物");
    }

    #[test]
    fn test_parse_entities_flat_array() {
        let json = serde_json::json!([
            { "name": "实体A", "type": "人物" }
        ]);
        let result = parse_entities(&json).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_parse_entities_empty_name() {
        let json = serde_json::json!({
            "entities": [
                { "name": "", "type": "人物" },
                { "name": "实体A", "type": "人物" }
            ]
        });
        let result = parse_entities(&json).unwrap();
        assert_eq!(result.len(), 1);
    }

    // ── 边界测试 ──

    #[test]
    fn test_unify_entities_empty() {
        let (result, stats, map) = unify_entities(&[]);
        assert!(result.is_empty());
        assert_eq!(stats.original_count, 0);
        assert_eq!(stats.unified_count, 0);
        assert!(map.is_empty());
    }

    #[test]
    fn test_unify_entities_single() {
        let entities = vec![Entity {
            name: "实体A".to_string(),
            entity_type: "类型A".to_string(),
            context: "描述A".to_string(),
        }];
        let (result, stats, map) = unify_entities(&entities);
        assert_eq!(result.len(), 1);
        assert_eq!(stats.merged_groups, 0);
        assert_eq!(map.get("实体A"), Some(&"实体A".to_string()));
    }

    #[test]
    fn test_unify_entities_special_chars() {
        // 特殊字符名称不应崩溃
        let entities = vec![
            Entity {
                name: "实体\n换行".to_string(),
                entity_type: "类型A".to_string(),
                context: "".to_string(),
            },
            Entity {
                name: "实体\t制表".to_string(),
                entity_type: "类型A".to_string(),
                context: "".to_string(),
            },
        ];
        let (_result, stats, _map) = unify_entities(&entities);
        // 编辑距离可能不够高，不应合并
        assert_eq!(stats.merged_groups, 0);
    }

    #[test]
    fn test_name_similarity_boundary() {
        // 编辑距离阈值为0.85，测试边界值
        // "abcdef" vs "abcdeg" -> 距离1/长度6 -> 相似度0.833
        assert!(name_similarity("abcdef", "abcdeg") < 0.85);
        // "abcde" vs "abcdf" -> 距离1/长度5 -> 相似度0.8
        assert!(name_similarity("abcde", "abcdf") < 0.85);
        // "abc" vs "abc" -> 距离0 -> 相似度1.0
        assert_eq!(name_similarity("abc", "abc"), 1.0);
    }

    #[test]
    fn test_dedup_entities_case_insensitive() {
        let entities = vec![
            Entity {
                name: "实体A".to_string(),
                entity_type: "类型A".to_string(),
                context: "".to_string(),
            },
            Entity {
                name: "实体a".to_string(),
                entity_type: "类型A".to_string(),
                context: "".to_string(),
            },
            Entity {
                name: "实体B".to_string(),
                entity_type: "类型A".to_string(),
                context: "".to_string(),
            },
        ];
        let result = dedup_entities(&entities);
        // "实体A" 和 "实体a" 大小写不同但应去重
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_normalize_name_empty() {
        assert_eq!(normalize_name(""), "");
    }

    #[test]
    fn test_levenshtein_distance_boundary() {
        assert_eq!(levenshtein_distance("", ""), 0);
        assert_eq!(levenshtein_distance("a", ""), 1);
        assert_eq!(levenshtein_distance("", "a"), 1);
        assert_eq!(levenshtein_distance("ab", "ba"), 2);
    }
}
