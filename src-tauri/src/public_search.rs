use crate::models::{SearchChannel, SourceEvidence, VerificationStatus};

// Shared discovery policy, not a second search client. Track-specific matching,
// contacts, materials and retry behavior remain in their existing owners.
pub const POLICY: &str = "Use Codex web search for accessible public recruitment information, including LinkedIn and Twitter / X posts. Do not install channel tools, connect social accounts, or bypass access restrictions. Follow leads to inspected official employer/institution/lab, funder or ATS pages. Record each source URL, checkedAt, evidenceType, channel and backend=codex_web_search. Public posts, Scholar/search snippets and inaccessible pages are discovery leads, not primary proof of a vacancy. State access, freshness and evidence limits; do not claim exhaustive coverage. Preserve secondary links alongside official evidence. Academic publications establish research direction, not hiring or funding. If primary evidence cannot be found, retain the opportunity as an unverified lead with uncertain availability and no contacts or materials; continue verification only within the requested scope.";

pub fn is_primary(source: &SourceEvidence) -> bool {
    if source.channel != SearchChannel::WebAts || source.evidence_type != "primary" { return false; }
    let Ok(url) = url::Url::parse(&source.url) else { return false; };
    let host = url.host_str().unwrap_or_default();
    // A mislabelled social/search result must not bypass the primary-source gate.
    !["linkedin.com", "lnkd.in", "twitter.com", "x.com", "t.co", "facebook.com", "google.com", "bing.com"]
        .iter().any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

pub fn verification(sources: &[SourceEvidence]) -> (VerificationStatus, SearchChannel, String) {
    if let Some(source) = sources.iter().find(|source| is_primary(source)) {
        return (VerificationStatus::Verified, source.channel.clone(), source.backend.clone());
    }
    let source = sources.first();
    (VerificationStatus::Unverified, source.map(|s| s.channel.clone()).unwrap_or_default(),
        source.map(|s| s.backend.clone()).unwrap_or_else(|| "unknown".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn social_and_scholar_cannot_be_primary_even_when_mislabelled() {
        for url in ["https://www.linkedin.com/posts/example", "https://x.com/example/status/1", "https://scholar.google.com/citations?user=example"] {
            let source = SourceEvidence { title:"Lead".into(),url:url.into(),checked_at:"2026-09-09T00:00:00Z".into(),
                evidence_type:"primary".into(),channel:SearchChannel::WebAts,backend:"codex_web_search".into() };
            assert_eq!(verification(&[source]).0,VerificationStatus::Unverified);
        }
        assert_eq!(verification(&[]).0,VerificationStatus::Unverified);
    }
}
