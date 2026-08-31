use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

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
    if let Ok(data) = serde_json::from_value::<CvData>(value.clone()) {
        validate(&data)?;
        return Ok(data)
    }
    convert_legacy_agent_shape(value)
}

pub(crate) fn completeness_score(data: &CvData) -> usize {
    let canonical = data.sections.iter().map(|section| canonical_section(&section.title))
        .collect::<std::collections::HashSet<_>>();
    let essential = [
        "research profile", "target alignment", "education", "selected research outputs",
        "selected research experience", "technical skills", "selected patents",
        "honors and academic service", "referees",
    ];
    essential.iter().filter(|title| canonical.contains(**title)).count() * 100
        + data.sections.iter().map(|section| section.entries.len()).sum::<usize>()
}

/// Keep the complete verified CV as the source of truth and allow a task to tailor
/// the headline and target alignment. A shorter Agent selection must never erase
/// education, publications, patents, service or referees from the master CV.
pub(crate) fn merge_preserving_baseline(baseline: &CvData, proposed: &CvData) -> CvData {
    let mut merged = baseline.clone();
    if !proposed.tagline.trim().is_empty() { merged.tagline = proposed.tagline.clone(); }

    let proposed_alignment = proposed.sections.iter()
        .find(|section| canonical_section(&section.title) == "target alignment")
        .cloned();
    if let Some(alignment) = proposed_alignment {
        if let Some(index) = merged.sections.iter().position(|section| canonical_section(&section.title) == "target alignment") {
            merged.sections[index] = alignment;
        } else {
            let insert_at = merged.sections.iter().position(|section| canonical_section(&section.title) == "research profile")
                .map(|index| index + 1).unwrap_or(0);
            merged.sections.insert(insert_at, alignment);
        }
    }

    for section in &proposed.sections {
        let key = canonical_section(&section.title);
        if key == "target alignment" { continue }
        let baseline_entries = merged.sections.iter()
            .filter(|candidate| canonical_section(&candidate.title) == key)
            .map(|candidate| candidate.entries.len()).sum::<usize>();
        if baseline_entries == 0 {
            merged.sections.push(section.clone());
        } else if section.entries.len() >= baseline_entries {
            let matching = merged.sections.iter().enumerate()
                .filter(|(_, candidate)| canonical_section(&candidate.title) == key)
                .map(|(index, _)| index).collect::<Vec<_>>();
            if matching.len() == 1 { merged.sections[matching[0]] = section.clone(); }
        }
    }
    merged
}

fn canonical_section(title: &str) -> String {
    title.trim().to_lowercase().replace("(continued)", "").trim().to_owned()
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
    fn incomplete_agent_selection_cannot_erase_master_cv_sections() -> Result<()> {
        let baseline = normalize(&json!({
            "schemaVersion":1,"name":"Hongbo Miao","tagline":"Master headline",
            "contact":"verified@example.com","affiliations":"HKU · HEU",
            "sections":[
                {"title":"Research Profile","entries":[{"key":"Focus","body":"Verified profile"}]},
                {"title":"Target Alignment","entries":[{"key":"Target","body":"Old target"}]},
                {"title":"Education","entries":[{"key":"PhD","body":"Doctoral education"},{"key":"BEng","body":"Bachelor education"}]},
                {"title":"Selected Patents","entries":[{"key":"Granted","body":"Verified patent"}]},
                {"title":"Referees","entries":[{"key":"HKU","body":"Verified referee"}]}
            ]
        }))?;
        let proposed = normalize(&json!({
            "schemaVersion":1,"name":"Hongbo Miao","tagline":"KAUST target headline",
            "contact":"verified@example.com","affiliations":"HKU · HEU",
            "sections":[
                {"title":"Target Alignment","entries":[{"key":"Target","body":"KAUST alignment"}]},
                {"title":"Education","entries":[{"key":"PhD","body":"Only one selected degree"}]}
            ]
        }))?;
        let merged = merge_preserving_baseline(&baseline, &proposed);
        assert_eq!(merged.tagline, "KAUST target headline");
        assert!(merged.sections.iter().any(|section| section.title == "Selected Patents"));
        assert!(merged.sections.iter().any(|section| section.title == "Referees"));
        assert_eq!(merged.sections.iter().find(|section| section.title == "Education").unwrap().entries.len(), 2);
        assert_eq!(merged.sections.iter().find(|section| section.title == "Target Alignment").unwrap().entries[0].body, "KAUST alignment");
        Ok(())
    }
}
