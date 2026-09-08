use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

const RESEARCH_PROFILE_TITLE: &str = "Research Profile";
const RESEARCH_OUTPUTS_TITLE: &str = "Selected Research Outputs";
const PATENTS_TITLE: &str = "Selected Patents";
const PROJECTS_TITLE: &str = "Selected Research Projects";
const EDUCATION_TITLE: &str = "Education & Current Stage";
const CAPABILITIES_TITLE: &str = "Technical Capabilities";
const HONORS_SERVICE_TITLE: &str = "Honors, Teaching & Service";
const REFERENCES_TITLE: &str = "References";

const STANDARD_SECTION_ORDER: [&str; 8] = [
    RESEARCH_PROFILE_TITLE,
    RESEARCH_OUTPUTS_TITLE,
    PATENTS_TITLE,
    PROJECTS_TITLE,
    EDUCATION_TITLE,
    CAPABILITIES_TITLE,
    HONORS_SERVICE_TITLE,
    REFERENCES_TITLE,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CvData {
    #[serde(default = "protocol_version", alias = "schema_version")]
    pub(crate) schema_version: u8,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) author_name: String,
    #[serde(default)]
    pub(crate) tagline: String,
    #[serde(default)]
    pub(crate) contact: String,
    #[serde(default)]
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
        "authorName": "candidate name exactly as it appears in publication or output author lists",
        "tagline": "targeted research headline",
        "contact": "email · phone",
        "affiliations": "current affiliations separated by ·",
        "sections": [
            {"title": RESEARCH_OUTPUTS_TITLE, "entries": [{"key": "year or status", "body": "complete verified publication or research-output entry"}]},
            {"title": PATENTS_TITLE, "entries": [{"key": "status", "body": "complete verified patent entry; include this section when the profile has usable patents"}]},
            {"title": PROJECTS_TITLE, "entries": [{"key": "short project label", "body": "complete verified target-relevant project entry"}]},
            {"title": EDUCATION_TITLE, "entries": [{"key": "date or stage", "body": "complete verified education or current-stage entry"}]},
            {"title": CAPABILITIES_TITLE, "entries": [{"key": "short capability label", "body": "complete verified methods, tools, or domain-capability entry"}]},
            {"title": HONORS_SERVICE_TITLE, "entries": [{"key": "short label", "body": "complete verified honor, teaching, reviewing, or service entry"}]},
            {"title": REFERENCES_TITLE, "entries": [
                {"key": "source CV reference", "body": "only supplied reference details; omit this section when absent"}
            ]}
        ]
    })
}

pub(crate) fn generation_policy_contract() -> Value {
    json!({
        "suggestedSectionOrder": STANDARD_SECTION_ORDER,
        "sectionOrder": "Current user instructions control section order, titles and selection. The current CV is a starting point, never a locked template.",
        "suggestedSectionNames": {
            "publications": RESEARCH_OUTPUTS_TITLE,
            "patents": PATENTS_TITLE,
            "projects": PROJECTS_TITLE,
            "education": EDUCATION_TITLE,
            "skills": CAPABILITIES_TITLE,
            "honors_and_service": HONORS_SERVICE_TITLE
        },
        "references": {
            "title": REFERENCES_TITLE,
            "mode": "fromSourceCv",
            "customizable": true,
            "fixedCount": false,
            "source": "Preserve references from the source CV by default; omit when absent. User customization may select, reorder or hide them. Never invent missing details."
        },
        "largeEntryStructure": {
            "source": "Source CV facts and current user instructions; legacy cv_structure.json is not a constraint",
            "preserveSectionOrder": false,
            "preserveEachSectionLargeEntryCount": false,
            "referencesCountAsLargeEntries": false,
            "tailoringAllowed": "Add, remove, rename and reorder sections or entries as requested, using source-backed facts. Never invent facts or make unrelated changes."
        },
        "contentQuality": {
            "targetRelevant": true,
            "sourceBackedOnly": true,
            "distinct": true,
            "rejectPlaceholdersAndGenericPadding": true
        },
        "customization": "User page count and enabled customization guide selection, structure, language and reference display. Never invent facts. No fixed section or entry counts apply, including from legacy settings."
    })
}

pub(crate) fn generation_rules() -> Vec<&'static str> {
    vec![
        "Use a CV structure suitable for the source CV, discipline, requested language, and target opportunity. Sample section titles and order are suggestions, not a mandatory template. Honor user-requested additions, removals, renamed sections and order. For revisions use the immutable current CV as the starting point, not a locked structure.",
        "Preserve important source-backed experience and tailor emphasis and detail to the target. Never freeze section or entry counts, even if legacy cvCustomization.preserveStructure is true.",
        "Include references present in the source CV by default; omit when absent. Enabled customization may select, reorder or hide references. No fixed count or mandatory role, institution, email or confirmation flag. Never invent missing details.",
        "Honor requested page count, language and customization. Automatic length should be readable and substantive; do not pad or invent material to fill pages.",
        "Treat explicit source CV statements as user-provided facts, not externally verified claims. Preserve dates, publication status, names and uncertainty accurately.",
        "Use the supplied publication-author form in cvData.authorName when available; do not infer an unfamiliar name convention.",
    ]
}

pub(crate) fn normalize_value(value: &Value) -> Result<Value> {
    serde_json::to_value(normalize(value)?).context("无法序列化规范化 CV 数据")
}

pub(crate) fn normalize_text(raw: &str) -> Result<String> {
    let value: Value = serde_json::from_str(raw).context("CV 结构化内容不是有效 JSON")?;
    Ok(serde_json::to_string_pretty(&normalize(&value)?)?)
}

pub(crate) fn normalize(value: &Value) -> Result<CvData> {
    let mut data = match serde_json::from_value::<CvData>(value.clone()) {
        Ok(data) => data,
        Err(_) => convert_legacy_agent_shape(value)?,
    };
    data.name = strip_emphasis_markers(&data.name);
    data.author_name = strip_emphasis_markers(&data.author_name);
    data.tagline = strip_emphasis_markers(&data.tagline);
    data.contact = strip_emphasis_markers(&data.contact);
    data.affiliations = strip_emphasis_markers(&data.affiliations);
    for section in &mut data.sections {
        section.title = strip_emphasis_markers(&section.title);
        for entry in &mut section.entries {
            entry.key = strip_emphasis_markers(&entry.key);
            entry.body = strip_emphasis_markers(&entry.body);
        }
    }
    if data.author_name.trim().is_empty() {
        data.author_name = inferred_publication_name(&data.name);
    }
    let data = normalize_structure(data)?;
    validate(&data)?;
    Ok(data)
}

fn strip_emphasis_markers(value: &str) -> String {
    value.replace("**", "").replace("__", "").trim().to_owned()
}

fn canonical_section(title: &str) -> String {
    title.trim().to_lowercase().replace("(continued)", "").trim().to_owned()
}

fn is_reference_section(title: &str) -> bool {
    matches!(canonical_section(title).as_str(),
        "references" | "referees" | "recommenders" | "professional references" |
        "academic references" | "推荐人" | "推荐人信息" | "推荐人联系方式")
}

fn inferred_publication_name(full_name: &str) -> String {
    let parts = full_name.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 2 { return full_name.trim().to_owned() }
    let surname = parts.last().copied().unwrap_or_default().trim_matches(',');
    let initial = parts.first().and_then(|part| part.chars().next());
    initial.map(|value| format!("{surname}, {value}.")).unwrap_or_else(|| full_name.trim().to_owned())
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

/// Normalization must not rewrite user-selected titles, order or entry counts.
/// Reject duplicates without merging, sorting or silently dropping content.
fn normalize_structure(data: CvData) -> Result<CvData> {
    let mut seen_sections = HashSet::<String>::new();
    for section in &data.sections {
        if !seen_sections.insert(canonical_section(&section.title)) {
            bail!("CV 重复包含章节“{}”；请使用唯一章节标题", section.title)
        }
    }
    Ok(data)
}

pub(crate) fn validate_generation_policy(
    data: &CvData,
    _master_profile: &Value,
) -> Result<()> {
    validate_substantive_entries(data)?;
    Ok(())
}

fn validate_substantive_entries(data: &CvData) -> Result<()> {
    const GENERIC_PADDING: [&str; 8] = [
        "lorem ipsum",
        "placeholder",
        "to be added",
        "additional relevant evidence",
        "other verified evidence",
        "target relevant evidence",
        "supporting detail",
        "miscellaneous experience",
    ];
    let mut seen_entries = HashSet::new();
    for section in &data.sections {
        for entry in &section.entries {
            let normalized_key = canonical_entry(&entry.key);
            let normalized_body = canonical_entry(&entry.body);
            if !is_reference_section(&section.title)
                && matches!(normalized_key.as_str(), "selected" | "item" | "additional" | "other" | "evidence")
            {
                bail!("CV 章节“{}”含有低价值通用标签“{}”", section.title, entry.key)
            }
            if GENERIC_PADDING.iter().any(|phrase| normalized_body.contains(phrase)) {
                bail!("CV 章节“{}”含有占位或通用凑数内容：{}", section.title, entry.body)
            }
            if !seen_entries.insert(format!("{normalized_key}:{normalized_body}")) {
                bail!(
                    "CV 章节“{}”重复包含条目“{}”；请删除重复内容或明确区分不同成果",
                    section.title,
                    entry.key,
                )
            }
        }
    }
    Ok(())
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
    let alignment = strip_emphasis_markers(object.get("alignment").and_then(Value::as_str).unwrap_or_default());
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
    push_object_entries(
        &mut sections,
        object.get("selected_references")
            .or_else(|| object.get("references"))
            .or_else(|| object.get("referees"))
            .or_else(|| object.get("recommendations"))
            .or_else(|| object.get("refs")),
        "References",
        &["name", "person", "referee", "recommender", "referees"],
        &[
            "name",
            "person",
            "referee",
            "recommender",
            "title",
            "position",
            "institution",
            "organization",
            "email",
            "phone",
            "contact",
            "relationship",
        ],
    );
    let data = CvData {
        schema_version: protocol_version(),
        name,
        author_name: string_at(candidate, "publication_name").unwrap_or_default(),
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
        let (key, body) = match item {
            Value::String(value) => {
                (first_text(&Map::new(), key_fields).unwrap_or_else(|| "Selected".into()), strip_emphasis_markers(value))
            }
            Value::Object(item) => {
                let key = first_text(item, key_fields).unwrap_or_else(|| "Selected".into());
                let body = body_fields.iter().filter_map(|field| item.get(*field))
                    .map(value_text).filter(|part| !part.is_empty()).collect::<Vec<_>>().join(". ");
                (key, body)
            }
            _ => return None,
        };
        (!body.is_empty()).then(|| CvEntry { key, body })
    }).collect::<Vec<_>>();
    if !entries.is_empty() {
        sections.push(CvSection { title: title.into(), entries });
    }
}

fn string_at(object: Option<&Map<String, Value>>, key: &str) -> Option<String> {
    object?
        .get(key)
        .and_then(Value::as_str)
        .map(strip_emphasis_markers)
        .filter(|value| !value.is_empty())
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
        Value::String(value) => strip_emphasis_markers(value),
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

    fn policy_profile() -> Value {
        json!({
            "publications":[{"title":"Publication"}],
            "patents":[{"title":"Patent"}],
            "projects":[{"title":"Project"}],
            "education":[{"degree":"PhD"}],
            "skills":{"methods":["Method"]},
            "honors_and_service":{"honors":["Honor"]},
            "referees":[
                {"name":"Prof. Ada One","role":"Professor","institution":"University One","email":"ada.one@example.org","claim_status":"usable"},
                {"name":"Dr. Ben Two","role":"Associate Professor","institution":"University Two","email":"ben.two@example.org","claim_status":"verified"},
                {"name":"Prof. Cy Three","role":"Professor","institution":"University Three","email":"cy.three@example.org","claim_status":"approved"}
            ]
        })
    }

    fn evidence_entries(prefix: &str, count: usize) -> Vec<Value> {
        (0..count).map(|index| json!({
            "key": format!("{prefix} {index}"),
            "body": format!("Distinct verified {prefix} evidence number {index} with specific methods, context, contribution, and outcome for the target role.")
        })).collect()
    }

    fn policy_cv() -> Result<CvData> {
        normalize(&json!({
            "schemaVersion":1,
            "name":"Alex Morgan",
            "authorName":"Morgan, A.",
            "tagline":"Targeted profile",
            "contact":"candidate@example.org",
            "affiliations":"Example Institute",
            "sections":[
                {"title":"Selected Publications","entries":evidence_entries("publication", 4)},
                {"title":"Patents","entries":evidence_entries("patent", 1)},
                {"title":"Research Experience","entries":evidence_entries("project", 6)},
                {"title":"Education","entries":evidence_entries("education", 2)},
                {"title":"Core Methods and Tools","entries":evidence_entries("capability", 4)},
                {"title":"Honours and Service","entries":evidence_entries("honor", 3)},
                {"title":"Referees","entries":[
                    {"key":"Prof. Ada One","body":"Prof. Ada One, Professor, University One, ada.one@example.org"},
                    {"key":"Dr. Ben Two","body":"Dr. Ben Two, Associate Professor, University Two, ben.two@example.org"},
                    {"key":"Prof. Cy Three","body":"Prof. Cy Three, Professor, University Three, cy.three@example.org"}
                ]}
            ]
        }))
    }

    #[test]
    fn accepts_canonical_camel_case() -> Result<()> {
        let data = normalize(&json!({
            "schemaVersion":1,"name":"Alex Morgan","authorName":"Morgan, A.","tagline":"Targeted profile",
            "contact":"candidate@example.org","affiliations":"Example Institute",
            "sections":[{"title":"Research","entries":[{"key":"Focus","body":"Verified research topic"}]}]
        }))?;
        assert_eq!(data.name, "Alex Morgan");
        assert_eq!(data.author_name, "Morgan, A.");
        Ok(())
    }

    #[test]
    fn converts_legacy_agent_selection_shape() -> Result<()> {
        let value = normalize_value(&json!({
            "schema_version":1,
            "candidate":{"full_name":"Alex Morgan","email":"candidate@example.org","current_roles":["Example Institute"]},
            "headline":["Target field","Relevant methods"],
            "alignment":"Verified alignment with the opportunity.",
            "selected_research":[{"title":"Research project","summary":"Verified method and result."}],
            "selected_publications":[{"title":"Paper","venue":"Example Journal","year":2025,"status":"published"}],
            "selected_skills":{"methods":["Method A","Method B"]}
        }))?;
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["name"], "Alex Morgan");
        assert!(value["sections"].as_array().is_some_and(|items| items.len() >= 3));
        Ok(())
    }

    #[test]
    fn accepts_legacy_references_shape() -> Result<()> {
        let value = normalize_value(&json!({
            "schema_version":1,
            "candidate":{"full_name":"Alex Morgan","email":"candidate@example.org","publication_name":"Morgan, A."},
            "selected_references":[
                {"name":"Dr. Li","position":"Professor","institution":"Beijing Univ.","email":"li@example.org"},
                "Prof. Chen, Dept. of AI"
            ]
        }))?;
        let sections = value["sections"].as_array().context("sections missing")?;
        let references = sections.iter().find(|section| section["title"].as_str().is_some_and(is_reference_section))
            .context("references section missing")?;
        assert_eq!(references["entries"].as_array().context("entries missing")?.len(), 2);
        Ok(())
    }

    #[test]
    fn strips_markdown_emphasis_markers() -> Result<()> {
        let value = normalize_value(&json!({
            "schemaVersion":1,
            "name":"Alex Morgan",
            "tagline":"Research **Focused** and __Strong__",
            "contact":"candidate@example.org",
            "affiliations":"Example Institute",
            "sections":[
                {"title":"Research","entries":[
                    {"key":"A","body":"Verified **methods** in AI."},
                    {"key":"B","body":"__Deep__ learning and **models**."}
                ]}
            ]
        }))?;
        assert_eq!(value["tagline"], "Research Focused and Strong");
        let sections = value["sections"].as_array().context("sections missing")?;
        let entries = sections[0]["entries"].as_array().context("entries missing")?;
        assert_eq!(entries[0]["body"], "Verified methods in AI.");
        assert_eq!(entries[1]["body"], "Deep learning and models.");
        Ok(())
    }

    #[test]
    fn repeated_sections_are_rejected_without_changing_large_entry_structure() -> Result<()> {
        let error = normalize(&json!({
            "schemaVersion":1,"name":"Alex Morgan","tagline":"Target headline",
            "contact":"verified@example.org","affiliations":"Example Institute",
            "sections":[
                {"title":"Selected Research Experience","entries":[
                    {"key":"One","body":"Verified qualitative analysis."},
                    {"key":"Duplicate","body":"Verified qualitative analysis"}
                ]},
                {"title":"Selected Research Experience (continued)","entries":[
                    {"key":"Two","body":"Independent field validation."}
                ]}
            ]
        })).unwrap_err();
        assert!(format!("{error:#}").contains("唯一章节标题"));
        Ok(())
    }

    #[test]
    fn normalization_preserves_user_titles_order_and_separate_disciplines() -> Result<()> {
        let raw = json!({"schemaVersion":1,"name":"Example Candidate","sections":[
            {"title":"Education","entries":[{"key":"Degree","body":"Documented doctoral education."}]},
            {"title":"Teaching","entries":[{"key":"Course","body":"Taught documented methods."}]},
            {"title":"Honors","entries":[{"key":"Award","body":"Received a documented award."}]},
            {"title":"Selected Publications","entries":[{"key":"Paper","body":"Published source-backed research."}]}
        ]});
        let data = normalize(&raw)?;
        assert_eq!(data.sections.iter().map(|s|s.title.as_str()).collect::<Vec<_>>(),
            vec!["Education","Teaching","Honors","Selected Publications"]);
        assert_eq!(normalize(&serde_json::to_value(&data)?)?,data);
        assert!(generation_policy_contract().get("standardSectionOrder").is_none());
        Ok(())
    }

    #[test]
    fn user_section_order_is_preserved() -> Result<()> {
        let data=policy_cv()?;
        let mut reordered=data.clone();
        let index=reordered.sections.iter().position(|s|s.title=="Education").unwrap();
        let education=reordered.sections.remove(index);
        reordered.sections.insert(0,education);
        let normalized=normalize(&serde_json::to_value(&reordered)?)?;
        assert_eq!(normalized.sections[0].title,"Education");
        validate_generation_policy(&normalized,&json!({}))?;
        assert_eq!(normalized.sections[0].title,"Education");
        Ok(())
    }

    #[test]
    fn generation_policy_allows_source_references_or_no_references() -> Result<()> {
        let original = policy_cv()?;
        for count in [0, 1, 2, 3, 4] {
            let mut data = original.clone();
            data.sections.retain(|section| !is_reference_section(&section.title));
            if count > 0 {
                data.sections.push(CvSection { title: REFERENCES_TITLE.into(), entries: (0..count).map(|n| CvEntry {
                    key: format!("Referee {n}"), body: format!("Source CV reference {n}"),
                }).collect() });
            }
            validate_generation_policy(&data, &json!({}))?;
        }
        Ok(())
    }

    #[test]
    fn generation_policy_rejects_generic_padding() -> Result<()> {
        let mut data = policy_cv()?;
        data.sections[0].entries[0].body = "Additional relevant evidence placeholder".into();
        let error = validate_generation_policy(&data, &policy_profile()).unwrap_err();
        assert!(format!("{error:#}").contains("占位或通用凑数内容"));
        Ok(())
    }

    #[test]
    fn content_policy_does_not_lock_entry_counts_or_section_titles() -> Result<()> {
        let data = policy_cv()?;
        validate_generation_policy(&data, &policy_profile())?;

        let mut removed = data.clone();
        removed.sections[0].entries.pop();
        validate_generation_policy(&removed, &policy_profile())?;

        let mut reordered = data;
        reordered.sections.swap(0, 1);
        validate_generation_policy(&reordered, &policy_profile())?;
        reordered.sections[0].title = "Replacement section".into();
        validate_generation_policy(&reordered, &policy_profile())?;
        Ok(())
    }

    #[test]
    fn references_can_follow_user_customized_order() -> Result<()> {
        let mut data = policy_cv()?;
        let references = data.sections.iter_mut().find(|section| is_reference_section(&section.title))
            .context("references missing")?;
        references.entries.swap(0, 1);
        validate_generation_policy(&data, &json!({}))?;
        Ok(())
    }
}
