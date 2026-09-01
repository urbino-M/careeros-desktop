use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CvData {
    #[serde(default = "protocol_version", alias = "schema_version")]
    pub(crate) schema_version: u8,
    pub(crate) name: String,
    pub(crate) tagline: String,
    pub(crate) contact: String,
    pub(crate) affiliations: String,
    pub(crate) sections: Vec<CvSection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CvSection {
    pub(crate) title: String,
    pub(crate) entries: Vec<CvEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CvEntry {
    pub(crate) key: String,
    pub(crate) body: String,
}

pub(crate) fn protocol_version() -> u8 { 1 }

pub(crate) fn contract() -> Value {
    json!({
        "schemaVersion": 1,
        "name": "candidate full name",
        "tagline": "targeted research headline",
        "contact": "email · phone",
        "affiliations": "current affiliations separated by ·",
        "sections": [{
            "title": "section title",
            "entries": [{"key": "date, status, or short label", "body": "complete factual entry"}]
        }]
    })
}

pub(crate) fn normalize_value(value: &Value) -> Result<Value> {
    serde_json::to_value(normalize(value)?).context("无法序列化规范化 CV 数据")
}

pub(crate) fn normalize_text(raw: &str) -> Result<String> {
    let value: Value = serde_json::from_str(raw).context("CV 结构化内容不是有效 JSON")?;
    Ok(serde_json::to_string_pretty(&normalize(&value)?)?)
}

pub(crate) fn normalize(value: &Value) -> Result<CvData> {
    let data = match serde_json::from_value::<CvData>(value.clone()) {
        Ok(data) => data,
        Err(_) => convert_legacy_agent_shape(value)?,
    };
    let data = deduplicate(data);
    validate(&data)?;
    Ok(data)
}

fn canonical_section(title: &str) -> String {
    title.trim().to_lowercase().replace("(continued)", "").trim().to_owned()
}

fn research_section_order(title: &str) -> Option<u8> {
    let title = canonical_section(title);
    if ["research output", "publication", "paper", "article"]
        .iter()
        .any(|label| title.contains(label))
    {
        Some(0)
    } else if title.contains("patent") || title.contains("intellectual property") {
        Some(1)
    } else if title.contains("project") {
        Some(2)
    } else {
        None
    }
}

fn order_research_sections(sections: &mut Vec<CvSection>) {
    let Some(first_research_position) = sections
        .iter()
        .position(|section| research_section_order(&section.title).is_some())
    else {
        return;
    };
    let insert_at = sections[..first_research_position]
        .iter()
        .filter(|section| research_section_order(&section.title).is_none())
        .count();
    let mut ordered = sections
        .iter()
        .filter(|section| research_section_order(&section.title).is_some())
        .cloned()
        .collect::<Vec<_>>();
    ordered.sort_by_key(|section| research_section_order(&section.title));
    let mut retained = std::mem::take(sections)
        .into_iter()
        .filter(|section| research_section_order(&section.title).is_none())
        .collect::<Vec<_>>();
    retained.splice(insert_at..insert_at, ordered);
    *sections = retained;
}

fn canonical_entry(body: &str) -> String {
    body.to_lowercase()
        .chars()
        .map(|character| if character.is_alphanumeric() { character } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Agent output is already the target-specific selection. Normalize repeated
/// continuation sections and exact repeated claims inside that one target only;
/// never borrow entries from another contact's CV.
fn deduplicate(mut data: CvData) -> CvData {
    let mut sections = Vec::<CvSection>::new();
    let mut section_positions = HashMap::<String, usize>::new();
    let mut seen_entries = HashSet::<String>::new();

    for mut section in data.sections {
        let section_key = canonical_section(&section.title);
        section.entries.retain(|entry| {
            let key = canonical_entry(&entry.body);
            !key.is_empty() && seen_entries.insert(key)
        });
        if section.entries.is_empty() {
            continue;
        }
        if let Some(index) = section_positions.get(&section_key).copied() {
            sections[index].entries.extend(section.entries);
        } else {
            section.title = section.title.replace(" (continued)", "").replace("(continued)", "");
            section_positions.insert(section_key, sections.len());
            sections.push(section);
        }
    }
    order_research_sections(&mut sections);
    data.sections = sections;
    data
}

fn validate(data: &CvData) -> Result<()> {
    if data.schema_version != protocol_version() {
        bail!("不支持的 CV schemaVersion：{}", data.schema_version)
    }
    if data.name.trim().is_empty() || data.sections.is_empty() {
        bail!("CV 结构化数据缺少姓名或章节")
    }
    for section in &data.sections {
        if section.title.trim().is_empty() || section.entries.is_empty() {
            bail!("CV 章节缺少标题或内容")
        }
        if section.entries.iter().any(|entry| entry.body.trim().is_empty()) {
            bail!("CV 章节包含空内容")
        }
    }
    Ok(())
}

fn convert_legacy_agent_shape(value: &Value) -> Result<CvData> {
    let object = value.as_object().context("Agent 返回的 CV 不是 JSON 对象")?;
    let candidate = object.get("candidate").and_then(Value::as_object);
    let name = string_at(candidate, "full_name")
        .or_else(|| string_at(Some(object), "name"))
        .unwrap_or_default();
    let headline = string_list(object.get("headline"));
    let alignment = object.get("alignment").and_then(Value::as_str).unwrap_or_default();
    let tagline = if headline.is_empty() {
        alignment.to_owned()
    } else {
        headline.join(" · ")
    };
    let contact = [string_at(candidate, "email"), string_at(candidate, "phone")]
        .into_iter().flatten().filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>().join(" · ");
    let affiliations = candidate
        .and_then(|item| item.get("current_roles"))
        .map(|value| string_list(Some(value))).unwrap_or_default().join(" · ");

    let mut sections = Vec::new();
    if !alignment.trim().is_empty() {
        sections.push(CvSection {
            title: "Target Alignment".into(),
            entries: vec![CvEntry { key: "Research fit".into(), body: alignment.into() }],
        });
    }
    push_object_entries(
        &mut sections,
        object.get("selected_education"),
        "Education",
        &["period", "degree", "field"],
        &["degree", "field", "institution", "period", "grade", "rank", "details"],
    );
    push_object_entries(
        &mut sections,
        object.get("selected_research"),
        "Selected Research Experience",
        &["title"],
        &["summary"],
    );
    push_object_entries(
        &mut sections,
        object.get("selected_publications"),
        "Selected Research Outputs",
        &["year", "status"],
        &["title", "venue", "year", "status", "authorship"],
    );
    if let Some(skills) = object.get("selected_skills").and_then(Value::as_object) {
        let entries = skills.iter().filter_map(|(key, value)| {
            let body = value_text(value);
            (!body.is_empty()).then(|| CvEntry { key: humanize(key), body })
        }).collect::<Vec<_>>();
        if !entries.is_empty() {
            sections.push(CvSection { title: "Technical Skills".into(), entries });
        }
    }
    let data = CvData {
        schema_version: protocol_version(),
        name,
        tagline: if tagline.trim().is_empty() { "Research Curriculum Vitae".into() } else { tagline },
        contact,
        affiliations,
        sections,
    };
    validate(&data).context("Agent 返回的旧版 CV 结构无法兼容转换")?;
    Ok(data)
}

fn push_object_entries(
    sections: &mut Vec<CvSection>,
    value: Option<&Value>,
    title: &str,
    key_fields: &[&str],
    body_fields: &[&str],
) {
    let entries = value.and_then(Value::as_array).into_iter().flatten().filter_map(|item| {
        let object = item.as_object()?;
        let key = first_text(object, key_fields).unwrap_or_else(|| "Selected".into());
        let body = body_fields.iter().filter_map(|field| object.get(*field))
            .map(value_text).filter(|part| !part.is_empty()).collect::<Vec<_>>().join(". ");
        (!body.is_empty()).then(|| CvEntry { key, body })
    }).collect::<Vec<_>>();
    if !entries.is_empty() {
        sections.push(CvSection { title: title.into(), entries });
    }
}

fn string_at(object: Option<&Map<String, Value>>, key: &str) -> Option<String> {
    object?.get(key).and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned)
}

fn first_text(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().filter_map(|key| object.get(*key)).map(value_text).find(|value| !value.is_empty())
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value.and_then(Value::as_array).into_iter().flatten().map(value_text).filter(|value| !value.is_empty()).collect()
}

fn value_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.trim().to_owned(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Array(values) => values.iter().map(value_text).filter(|value| !value.is_empty()).collect::<Vec<_>>().join(", "),
        Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn humanize(value: &str) -> String {
    value.split('_').filter(|part| !part.is_empty()).map(|part| {
        let mut chars = part.chars();
        chars.next().map(|first| first.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
    }).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_canonical_camel_case() -> Result<()> {
        let data = normalize(&json!({
            "schemaVersion":1,"name":"Hongbo Miao","tagline":"Marine AI",
            "contact":"urbinohbmiao@gmail.com","affiliations":"HKU · HEU",
            "sections":[{"title":"Research","entries":[{"key":"Focus","body":"Underwater acoustics"}]}]
        }))?;
        assert_eq!(data.name, "Hongbo Miao");
        Ok(())
    }

    #[test]
    fn converts_legacy_agent_selection_shape() -> Result<()> {
        let value = normalize_value(&json!({
            "schema_version":1,
            "candidate":{"full_name":"Hongbo Miao","email":"urbinohbmiao@gmail.com","current_roles":["HKU","HEU"]},
            "headline":["Underwater Acoustics","Marine Robotics"],
            "alignment":"Propagation-aware cooperative sensing.",
            "selected_research":[{"title":"Localization","summary":"Physics-informed matched-field processing."}],
            "selected_publications":[{"title":"Paper","venue":"JASA","year":2025,"status":"published"}],
            "selected_skills":{"programming":["Python","MATLAB"]}
        }))?;
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["name"], "Hongbo Miao");
        assert!(value["sections"].as_array().is_some_and(|items| items.len() >= 3));
        Ok(())
    }

    #[test]
    fn repeated_sections_and_claims_are_collapsed_within_one_target() -> Result<()> {
        let data = normalize(&json!({
            "schemaVersion":1,"name":"Hongbo Miao","tagline":"Target headline",
            "contact":"verified@example.com","affiliations":"HKU · HEU",
            "sections":[
                {"title":"Selected Research Experience","entries":[
                    {"key":"One","body":"Physics-informed localization."},
                    {"key":"Duplicate","body":"Physics informed localization"}
                ]},
                {"title":"Selected Research Experience (continued)","entries":[
                    {"key":"Two","body":"Marine robotics field validation."}
                ]}
            ]
        }))?;
        assert_eq!(data.sections.len(), 1);
        assert_eq!(data.sections[0].title, "Selected Research Experience");
        assert_eq!(data.sections[0].entries.len(), 2);
        Ok(())
    }

    #[test]
    fn outputs_and_patents_are_ordered_before_projects() -> Result<()> {
        let data = normalize(&json!({
            "schemaVersion":1,"name":"Hongbo Miao","tagline":"Target headline",
            "contact":"verified@example.com","affiliations":"HKU · HEU",
            "sections":[
                {"title":"Education","entries":[{"key":"Degree","body":"Verified education."}]},
                {"title":"Selected Research Projects","entries":[{"key":"Project","body":"Verified project."}]},
                {"title":"Technical Expertise","entries":[{"key":"Skill","body":"Verified skill."}]},
                {"title":"Selected Patents","entries":[{"key":"Patent","body":"Verified patent."}]},
                {"title":"Selected Publications","entries":[{"key":"Paper","body":"Verified publication."}]}
            ]
        }))?;
        let titles = data.sections.iter().map(|section| section.title.as_str()).collect::<Vec<_>>();
        assert_eq!(
            titles,
            vec![
                "Education",
                "Selected Publications",
                "Selected Patents",
                "Selected Research Projects",
                "Technical Expertise",
            ]
        );
        Ok(())
    }
}
