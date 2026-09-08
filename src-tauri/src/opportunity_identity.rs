//! Postdoc identity evidence. Pure, offline and shared by search and continuation.
//! Similarity can request review; it must never silently merge two postings.
use std::collections::BTreeSet;
use url::Url;

pub(super) struct Facts<'a> {
    pub organization: &'a str,
    pub title: &'a str,
    pub source: &'a str,
    pub external_id: Option<&'a str>,
    pub saved_key: Option<&'a str>,
    pub deadline: Option<&'a str>,
}

#[derive(Debug, PartialEq)]
pub(super) enum Match {
    Same,
    Different,
    Review(&'static str),
}

pub(super) fn canonical_source(source: &str) -> String {
    let Ok(mut url) = Url::parse(source) else {
        return source.trim().into();
    };
    url.set_fragment(None);
    let mut pairs: Vec<_> = url
        .query_pairs()
        .filter(|(key, _)| {
            !matches!(
                key.as_ref(),
                "utm_source"
                    | "utm_medium"
                    | "utm_campaign"
                    | "utm_term"
                    | "utm_content"
                    | "fbclid"
                    | "gclid"
            )
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    // Repeated query keys can be order-sensitive; never reorder those.
    if pairs.iter().map(|(k, _)| k).collect::<BTreeSet<_>>().len() == pairs.len() {
        pairs.sort();
    }
    url.set_query(None);
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    url.to_string()
}

fn workday(source: &str) -> Option<(String, String)> {
    let url = Url::parse(source).ok()?;
    let host = url.host_str()?;
    if !host.ends_with(".myworkdayjobs.com") {
        return None;
    }
    let parts: Vec<_> = host.split('.').collect();
    if parts.len() != 4 || !parts[1].starts_with("wd") {
        return None;
    }
    let tail = url.path_segments()?.filter(|v| !v.is_empty()).next_back()?;
    let id = tail.rsplit('_').next()?;
    if !id.starts_with('R') || id.len() < 5 || !id[1..].bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((format!("workday:{}", parts[0]), id.into()))
}

fn authority(source: &str) -> String {
    if let Some((scope, _)) = workday(source) {
        return scope;
    }
    Url::parse(source)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default()
}

fn url_id(source: &str) -> Option<String> {
    if let Some((_, id)) = workday(source) {
        return Some(id);
    }
    let url = Url::parse(source).ok()?;
    let values: Vec<_> = url
        .query_pairs()
        .filter(|(k, v)| {
            !v.is_empty()
                && matches!(
                    k.as_ref(),
                    "jobId" | "job_id" | "requisitionId" | "postingId"
                )
        })
        .map(|(_, v)| v.into_owned())
        .collect();
    (values.len() == 1).then(|| values[0].clone())
}

fn official(f: &Facts<'_>) -> Option<(String, bool)> {
    if let Some(id) = f.external_id.map(str::trim).filter(|v| !v.is_empty()) {
        return Some((id.into(), false));
    }
    if let Some(id) = url_id(f.source) {
        return Some((id, false));
    }
    let key = f.saved_key?;
    if let Some(legacy) = key.strip_prefix("external:") {
        return Some((legacy.into(), true));
    }
    let encoded = key.strip_prefix("external-v2:")?;
    let parts: Vec<String> = serde_json::from_str(encoded).ok()?;
    parts.get(2).cloned().map(|id| (id, false))
}

pub(super) fn inconsistent_id(f: &Facts<'_>) -> bool {
    f.external_id
        .zip(url_id(f.source))
        .is_some_and(|(explicit, from_url)| {
            if workday(f.source).is_some() {
                !explicit.trim().eq_ignore_ascii_case(&from_url)
            } else {
                explicit.trim() != from_url
            }
        })
}

fn same_id(a: &Facts<'_>, b: &Facts<'_>) -> Option<bool> {
    let ((a_id, a_legacy), (b_id, b_legacy)) = (official(a)?, official(b)?);
    Some(if a_legacy || b_legacy {
        normalize(&a_id) == normalize(&b_id)
    } else if workday(a.source).is_some() && workday(b.source).is_some() {
        a_id.eq_ignore_ascii_case(&b_id)
    } else {
        a_id == b_id
    })
}

pub(super) fn conflicts(a: &Facts<'_>, b: &Facts<'_>) -> bool {
    authority(a.source) == authority(b.source) && same_id(a, b) == Some(false)
}

pub(super) fn key(f: &Facts<'_>) -> String {
    if let Some((id, _)) = official(f) {
        let id = if workday(f.source).is_some() {
            id.to_ascii_uppercase()
        } else {
            id
        };
        return format!(
            "external-v2:{}",
            serde_json::to_string(&[authority(f.source), normalize(f.organization), id]).unwrap()
        );
    }
    format!(
        "url-v2:{}",
        serde_json::to_string(&[normalize(f.organization), canonical_source(f.source)]).unwrap()
    )
}

pub(super) fn compare(a: &Facts<'_>, b: &Facts<'_>, shared_contact: bool) -> Match {
    if inconsistent_id(a) || inconsistent_id(b) {
        return Match::Review("返回的岗位编号与来源网址编号冲突");
    }
    if conflicts(a, b) {
        return Match::Different;
    }
    let org = same_organization(a.organization, b.organization);
    let title = similar_title(a.title, b.title) || normalize(a.title) == normalize(b.title);
    let source = canonical_source(a.source) == canonical_source(b.source);
    let same_authority =
        !authority(a.source).is_empty() && authority(a.source) == authority(b.source);
    let official_match = same_authority && same_id(a, b) == Some(true);
    let new_cycle = a
        .deadline
        .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .zip(
            b.deadline
                .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()),
        )
        .is_some_and(|(a, b)| (a - b).num_days().abs() > 180);
    if (source || official_match) && new_cycle {
        return Match::Review("同一来源可能被用于新一轮招聘，截止日期相隔超过半年");
    }
    // A Workday tenant + requisition identifies the posting across locale/slug/shard changes.
    if official_match && (org || workday(a.source).is_some() && workday(b.source).is_some()) {
        return Match::Same;
    }
    if source && org {
        if official(a).is_none() && official(b).is_none() && !title {
            return Match::Review("同一网页出现不同岗位标题，不能把招聘列表页当成唯一岗位");
        }
        return Match::Same;
    }
    if www_alias(a.source, b.source) && org && similar_title(a.title, b.title) && shared_contact {
        return Match::Same;
    }
    if source
        || official_match
        || (org && same_id(a, b) == Some(true))
        || (org && title && (shared_contact || similar_title(a.title, b.title)))
    {
        return Match::Review("来源、岗位编号或研究主题与已有机会相似，但身份依据不足");
    }
    Match::Different
}

pub(super) fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

pub(super) fn same_organization(a: &str, b: &str) -> bool {
    let (a_full, b_full) = (normalize(a), normalize(b));
    if a_full == b_full {
        return true;
    }
    if a_full.len() >= 12 && a_full == normalize(b.split(',').next().unwrap_or(b)) {
        return true;
    }
    if b_full.len() >= 12 && b_full == normalize(a.split(',').next().unwrap_or(a)) {
        return true;
    }
    // Only explicit aliases in the supplied name, never guessed abbreviations.
    let declared = |long: &str, short: &str| {
        long.rsplit_once('(').is_some_and(|(_, tail)| {
            tail.strip_suffix(')')
                .is_some_and(|alias| alias.len() >= 3 && normalize(alias) == normalize(short))
        })
    };
    declared(a, b) || declared(b, a)
}

pub(super) fn www_alias(a: &str, b: &str) -> bool {
    let (Ok(mut a), Ok(mut b)) = (
        Url::parse(&canonical_source(a)),
        Url::parse(&canonical_source(b)),
    ) else {
        return false;
    };
    let (Some(ah), Some(bh)) = (a.host_str(), b.host_str()) else {
        return false;
    };
    if ah == bh || ah.strip_prefix("www.").unwrap_or(ah) != bh.strip_prefix("www.").unwrap_or(bh) {
        return false;
    }
    let host = ah.strip_prefix("www.").unwrap_or(ah).to_owned();
    a.set_host(Some(&host)).is_ok() && b.set_host(Some(&host)).is_ok() && a == b
}

pub(super) fn similar_title(a: &str, b: &str) -> bool {
    let words = |s: &str| {
        normalize(s)
            .split('-')
            .filter(|w| w.len() > 3)
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    };
    let (a, b) = (words(a), words(b));
    let shorter = a.len().min(b.len());
    shorter >= 6 && a.intersection(&b).count() * 4 >= shorter * 3
}

#[cfg(test)]
mod tests {
    use super::*;
    fn facts(source: &str) -> Facts<'_> {
        Facts {
            organization: "Example University",
            title: "Postdoctoral Fellowship in Distributed Fiber Sensing and Machine Learning",
            source,
            external_id: None,
            saved_key: None,
            deadline: None,
        }
    }
    #[test]
    fn query_order_and_tracking_are_equivalent_but_meaningful_values_are_not() {
        let a = facts("https://example.edu/Job/AbC?type=postdoc&jobId=A&utm_source=x");
        let b = facts("https://example.edu/Job/AbC?jobId=A&type=postdoc#top");
        assert_eq!(compare(&a, &b, false), Match::Same);
        let c = facts("https://example.edu/Job/AbC?jobId=a&type=postdoc");
        assert_eq!(compare(&a, &c, true), Match::Different);
        assert_ne!(
            canonical_source("https://e.org/jobs/?id=1&id=2"),
            canonical_source("https://e.org/jobs/?id=2&id=1")
        );
        assert_ne!(
            canonical_source("https://e.org/jobs/AbC"),
            canonical_source("https://e.org/jobs/abc")
        );
    }
    #[test]
    fn workday_identity_survives_translation_and_locale_but_not_tenant_or_requisition_changes() {
        let a = facts(
            "https://university.wd3.myworkdayjobs.com/Careers/job/Main/Research-Fellow_R00025405",
        );
        let mut b = facts(
            "https://university.wd5.myworkdayjobs.com/en-US/Careers/job/Different-Slug_R00025405",
        );
        b.organization = "Translated University Name";
        assert_eq!(compare(&a, &b, false), Match::Same);
        let c = facts("https://university.wd3.myworkdayjobs.com/Careers/job/Research_R00025407");
        assert_eq!(compare(&a, &c, true), Match::Different);
        let d = facts("https://another.wd3.myworkdayjobs.com/Careers/job/Research_R00025405");
        assert_ne!(compare(&a, &d, true), Match::Same);
        assert_ne!(key(&a), key(&d));
        b.external_id = Some("R00000000");
        assert!(inconsistent_id(&b));
    }
    #[test]
    fn ids_are_namespaced_and_legacy_keys_are_readable() {
        let mut a = facts("https://one.example/jobs/1");
        a.external_id = Some("JOB-12");
        let mut b = facts("https://two.example/jobs/1");
        b.external_id = Some("JOB-12");
        assert_ne!(key(&a), key(&b));
        assert_ne!(compare(&a, &b, true), Match::Same);
        b.source = a.source;
        b.organization = "Another University";
        assert_ne!(key(&a), key(&b));
        b.organization = a.organization;
        b.external_id = None;
        b.saved_key = Some("external:job-12");
        assert_eq!(compare(&a, &b, false), Match::Same);
        let saved = key(&a);
        b.saved_key = Some(&saved);
        assert_eq!(compare(&a, &b, false), Match::Same);
    }
    #[test]
    fn aliases_require_evidence_and_same_supervisor_is_not_same_posting() {
        let a = facts("https://example.edu/jobs.html");
        let b = facts("https://www.example.edu/jobs.html");
        assert_eq!(compare(&a, &b, true), Match::Same);
        assert_ne!(compare(&a, &b, false), Match::Same);
        let mut c = facts("https://example.edu/different");
        c.title = "Postdoctoral research in unrelated archival history";
        assert_eq!(compare(&a, &c, true), Match::Different);
        c.title = a.title;
        assert!(matches!(compare(&a, &c, true), Match::Review(_)));
        assert!(same_organization("Example University (EXU)", "EXU"));
        assert!(!same_organization("Example University", "EXU"));
        assert!(!same_organization(
            "Example University, North",
            "Example University, South"
        ));
    }
    #[test]
    fn generic_listing_and_reused_annual_posting_require_review() {
        let mut a = facts("https://example.edu/jobs.html");
        let mut b = facts(a.source);
        b.title = "Postdoctoral position in comparative medieval history";
        assert!(matches!(compare(&a, &b, true), Match::Review(_)));
        b.title = a.title;
        a.deadline = Some("2026-01-01");
        b.deadline = Some("2027-01-01");
        assert!(matches!(compare(&a, &b, true), Match::Review(_)));
    }
}
