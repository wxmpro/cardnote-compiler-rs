use serde_json::Value;

use crate::config::doc_limits_for;
use crate::error::Result;
use crate::models::{Entity, KnowledgeGraph, LlmMessage, Relation};
use crate::stages::common::{ChatFn, ChatJsonFn};

/// 构建知识图谱
pub async fn build_graph(
    document: &str,
    entities: &[Entity],
    _call_llm: impl ChatFn,
    call_llm_json: impl ChatJsonFn,
    load_prompt: &(dyn Fn(&str) -> Result<String> + Send + Sync),
) -> Result<KnowledgeGraph> {
    let prompt_template = load_prompt("relation_graph")?;
    let entities_text: Vec<String> = entities
        .iter()
        .map(|e| format!("- {} ({})", e.name, e.entity_type))
        .collect();

    let prompt = prompt_template
        .replace("{document}", document)
        .replace("{entities}", &entities_text.join("\n"));

    let system =
        "你是一位知识图谱专家，擅长发现实体之间的关系。请严格按 JSON 格式输出。".to_string();

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
            Some(doc_limits_for(document.chars().count()).graph_output as u32),
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("    ⚠ 图谱 LLM 调用失败，返回空关系: {}", e);
            return Ok(KnowledgeGraph {
                entities: entities.to_vec(),
                relations: Vec::new(),
            });
        }
    };

    match parse_graph(&response, entities) {
        Ok(graph) => Ok(graph),
        Err(e) => {
            eprintln!("    ⚠ 图谱解析失败，返回空关系: {}", e);
            Ok(KnowledgeGraph {
                entities: entities.to_vec(),
                relations: Vec::new(),
            })
        }
    }
}

/// 关系去重
pub fn dedup_relations(relations: &[Relation]) -> Vec<Relation> {
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for rel in relations {
        let key = format!(
            "{}:{}:{}",
            rel.source.to_lowercase(),
            rel.relation_type.to_lowercase(),
            rel.target.to_lowercase()
        );
        if seen.insert(key) {
            unique.push(rel.clone());
        }
    }
    unique
}

/// 更新关系端点（实体统一后的名称映射）
///
/// 如果关系的 source 或 target 在实体统一中被合并，
/// 将端点名称更新为统一后的名称。
pub fn update_relation_endpoints(
    relations: &[Relation],
    name_map: &std::collections::HashMap<String, String>,
) -> Vec<Relation> {
    let mut updated = Vec::with_capacity(relations.len());
    for rel in relations {
        let new_source = name_map
            .get(&rel.source)
            .cloned()
            .unwrap_or_else(|| rel.source.clone());
        let new_target = name_map
            .get(&rel.target)
            .cloned()
            .unwrap_or_else(|| rel.target.clone());
        updated.push(Relation {
            source: new_source,
            target: new_target,
            relation_type: rel.relation_type.clone(),
            evidence: rel.evidence.clone(),
        });
    }
    updated
}

/// 合并相同关系（合并 evidence）
///
/// 对于相同 source-target-type 的关系，合并它们的 evidence，
/// 去重后连接为一条关系。
pub fn merge_relations(relations: &[Relation]) -> Vec<Relation> {
    use std::collections::HashMap;

    let mut groups: HashMap<String, Vec<&Relation>> = HashMap::new();
    for rel in relations {
        let key = format!(
            "{}:{}:{}",
            rel.source.to_lowercase(),
            rel.relation_type.to_lowercase(),
            rel.target.to_lowercase()
        );
        groups.entry(key).or_default().push(rel);
    }

    let mut result = Vec::with_capacity(groups.len());
    for group in groups.values() {
        if group.len() == 1 {
            result.push(group[0].clone());
        } else {
            // 合并 evidence
            let mut evidences: Vec<String> = Vec::new();
            for rel in group {
                if !rel.evidence.is_empty() && !evidences.contains(&rel.evidence) {
                    evidences.push(rel.evidence.clone());
                }
            }
            let merged_evidence = if evidences.is_empty() {
                String::new()
            } else if evidences.len() == 1 {
                evidences[0].clone()
            } else {
                evidences.join("；")
            };
            result.push(Relation {
                source: group[0].source.clone(),
                target: group[0].target.clone(),
                relation_type: group[0].relation_type.clone(),
                evidence: merged_evidence,
            });
        }
    }
    result
}

/// 解析图谱 JSON
fn parse_graph(value: &Value, existing_entities: &[Entity]) -> Result<KnowledgeGraph> {
    let relations_arr = crate::stages::common::extract_json_array(value, "relations")?;

    let mut relations = Vec::new();
    for item in relations_arr {
        let source = item
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let target = item
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if source.is_empty() || target.is_empty() {
            continue;
        }

        let relation_type = item
            .get("relation_type")
            .or_else(|| item.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("相关")
            .to_string();
        let evidence = item
            .get("evidence")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        relations.push(Relation {
            source,
            target,
            relation_type,
            evidence,
        });
    }

    Ok(KnowledgeGraph {
        entities: existing_entities.to_vec(),
        relations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedup_relations() {
        let relations = vec![
            Relation {
                source: "Alice".to_string(),
                target: "Bob".to_string(),
                relation_type: "朋友".to_string(),
                evidence: "".to_string(),
            },
            Relation {
                source: "alice".to_string(),
                target: "bob".to_string(),
                relation_type: "朋友".to_string(),
                evidence: "新证据".to_string(),
            },
            Relation {
                source: "Alice".to_string(),
                target: "Charlie".to_string(),
                relation_type: "朋友".to_string(),
                evidence: "".to_string(),
            },
        ];
        let result = dedup_relations(&relations);
        assert_eq!(result.len(), 2);
        // 保留第一个（新证据不应覆盖）
        assert_eq!(result[0].evidence, "");
    }

    #[test]
    fn test_parse_graph_nested_object() {
        let json = serde_json::json!({
            "relations": [
                { "source": "Alice", "target": "Bob", "relation_type": "朋友" },
                { "source": "Bob", "target": "Charlie", "type": "同事", "evidence": "同部门" }
            ]
        });
        let entities = vec![Entity {
            name: "Alice".to_string(),
            entity_type: "人物".to_string(),
            context: "".to_string(),
        }];
        let result = parse_graph(&json, &entities).unwrap();
        assert_eq!(result.relations.len(), 2);
        assert_eq!(result.relations[0].relation_type, "朋友");
        assert_eq!(result.relations[1].relation_type, "同事");
        assert_eq!(result.relations[1].evidence, "同部门");
        assert_eq!(result.entities.len(), 1);
    }

    #[test]
    fn test_parse_graph_flat_array() {
        let json = serde_json::json!([
            { "source": "A", "target": "B", "relation_type": "相关" }
        ]);
        let result = parse_graph(&json, &[]).unwrap();
        assert_eq!(result.relations.len(), 1);
    }

    #[test]
    fn test_parse_graph_empty_source_target() {
        let json = serde_json::json!({
            "relations": [
                { "source": "", "target": "Bob", "relation_type": "朋友" },
                { "source": "Alice", "target": "", "relation_type": "朋友" },
                { "source": "Alice", "target": "Bob", "relation_type": "朋友" }
            ]
        });
        let result = parse_graph(&json, &[]).unwrap();
        assert_eq!(result.relations.len(), 1);
    }

    #[test]
    fn test_parse_graph_missing_relations() {
        let json = serde_json::json!({ "other": "data" });
        let result = parse_graph(&json, &[]);
        assert!(result.is_err());
    }

    // ── 边界测试 ──

    #[test]
    fn test_parse_graph_empty() {
        let json = serde_json::json!({ "relations": [] });
        let result = parse_graph(&json, &[]).unwrap();
        assert!(result.relations.is_empty());
        assert!(result.entities.is_empty());
    }

    #[test]
    fn test_parse_graph_self_reference() {
        let json = serde_json::json!({
            "relations": [
                { "source": "A", "target": "A", "relation": "自引用" }
            ]
        });
        let result = parse_graph(&json, &[]).unwrap();
        assert_eq!(result.relations.len(), 1);
        assert_eq!(result.relations[0].source, "A");
        assert_eq!(result.relations[0].target, "A");
    }

    #[test]
    fn test_dedup_relations_empty() {
        let result = dedup_relations(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_dedup_relations_single() {
        let relations = vec![Relation {
            source: "A".to_string(),
            target: "B".to_string(),
            relation_type: "相关".to_string(),
            evidence: "证据A".to_string(),
        }];
        let result = dedup_relations(&relations);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_update_relation_endpoints_empty() {
        let name_map = std::collections::HashMap::new();
        let result = update_relation_endpoints(&[], &name_map);
        assert!(result.is_empty());
    }

    #[test]
    fn test_merge_relations_empty() {
        let result = merge_relations(&[]);
        assert!(result.is_empty());
    }
}
