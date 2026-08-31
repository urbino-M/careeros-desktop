// V1 routes through OpenAI. These adapter types are deliberately compiled now
// so future providers can plug in without touching task or application code.
#![allow(dead_code)]

use anyhow::{bail, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionMode {
    NativeResponses,
    InternalGateway,
    ExternalGateway,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilities {
    pub reasoning: bool,
    pub tools: bool,
    pub vision: bool,
    pub streaming: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterModel {
    pub id: String,
    pub display_name: String,
    pub capabilities: ModelCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountValidation {
    pub valid: bool,
    pub account_label: Option<String>,
    pub message: String,
}

#[async_trait]
pub trait ModelProviderAdapter: Send + Sync {
    fn provider_id(&self) -> &'static str;
    fn connection_mode(&self) -> ConnectionMode;
    async fn validate_account(&self, account_reference: &str) -> Result<AccountValidation>;
    async fn list_models(&self, account_reference: Option<&str>) -> Result<Vec<AdapterModel>>;
    fn render_codex_config(&self, model: &str, account_reference: Option<&str>) -> Result<String>;
    fn capabilities(&self, model: &str) -> Result<ModelCapabilities>;
}

pub struct OpenAiCodexAdapter;

#[async_trait]
impl ModelProviderAdapter for OpenAiCodexAdapter {
    fn provider_id(&self) -> &'static str {
        "openai"
    }

    fn connection_mode(&self) -> ConnectionMode {
        ConnectionMode::NativeResponses
    }

    async fn validate_account(&self, account_reference: &str) -> Result<AccountValidation> {
        Ok(AccountValidation {
            valid: !account_reference.trim().is_empty(),
            account_label: None,
            message: if account_reference.trim().is_empty() {
                "尚未连接 OpenAI 账号".into()
            } else {
                "账号凭据已保存；执行任务前会由 Codex 再次校验".into()
            },
        })
    }

    async fn list_models(&self, _account_reference: Option<&str>) -> Result<Vec<AdapterModel>> {
        Ok([("gpt-5.6-sol", "Sol · 最高质量"), ("gpt-5.6-terra", "Terra · 平衡"), ("gpt-5.6-luna", "Luna · 快速")]
            .into_iter()
            .map(|(id, display_name)| AdapterModel {
                id: id.into(),
                display_name: display_name.into(),
                capabilities: ModelCapabilities {
                    reasoning: true,
                    tools: true,
                    vision: true,
                    streaming: true,
                },
            })
            .collect())
    }

    fn render_codex_config(&self, model: &str, _account_reference: Option<&str>) -> Result<String> {
        if !model.starts_with("gpt-") {
            bail!("OpenAI 模型标识无效")
        }
        Ok(format!("model = {model:?}\nmodel_provider = \"openai\"\n"))
    }

    fn capabilities(&self, model: &str) -> Result<ModelCapabilities> {
        if !model.starts_with("gpt-") {
            bail!("OpenAI 模型标识无效")
        }
        Ok(ModelCapabilities {
            reasoning: true,
            tools: true,
            vision: true,
            streaming: true,
        })
    }
}

pub struct ReservedAdapter {
    provider_id: &'static str,
    mode: ConnectionMode,
}

impl ReservedAdapter {
    pub fn new(provider_id: &'static str, mode: ConnectionMode) -> Self {
        Self { provider_id, mode }
    }
}

#[async_trait]
impl ModelProviderAdapter for ReservedAdapter {
    fn provider_id(&self) -> &'static str {
        self.provider_id
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.mode.clone()
    }

    async fn validate_account(&self, _account_reference: &str) -> Result<AccountValidation> {
        bail!("该服务商是未来扩展入口，当前版本未启用")
    }

    async fn list_models(&self, _account_reference: Option<&str>) -> Result<Vec<AdapterModel>> {
        Ok(Vec::new())
    }

    fn render_codex_config(&self, _model: &str, _account_reference: Option<&str>) -> Result<String> {
        bail!("该服务商是未来扩展入口，当前版本未启用")
    }

    fn capabilities(&self, _model: &str) -> Result<ModelCapabilities> {
        bail!("该服务商是未来扩展入口，当前版本未启用")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeDeepSeekAdapter;

    #[async_trait]
    impl ModelProviderAdapter for FakeDeepSeekAdapter {
        fn provider_id(&self) -> &'static str { "fake-deepseek" }
        fn connection_mode(&self) -> ConnectionMode { ConnectionMode::InternalGateway }
        async fn validate_account(&self, _: &str) -> Result<AccountValidation> {
            Ok(AccountValidation { valid: true, account_label: Some("test".into()), message: "ok".into() })
        }
        async fn list_models(&self, _: Option<&str>) -> Result<Vec<AdapterModel>> {
            Ok(vec![AdapterModel { id: "deepseek-test".into(), display_name: "Fake".into(), capabilities: self.capabilities("deepseek-test")? }])
        }
        fn render_codex_config(&self, model: &str, _: Option<&str>) -> Result<String> {
            Ok(format!("model = {model:?}\nbase_url = \"http://127.0.0.1:9\"\n"))
        }
        fn capabilities(&self, _: &str) -> Result<ModelCapabilities> {
            Ok(ModelCapabilities { reasoning: true, tools: true, vision: false, streaming: true })
        }
    }

    #[tokio::test]
    async fn fake_adapter_proves_provider_extension_without_business_modules() {
        let adapter: Box<dyn ModelProviderAdapter> = Box::new(FakeDeepSeekAdapter);
        assert_eq!(adapter.connection_mode(), ConnectionMode::InternalGateway);
        assert!(adapter.validate_account("secret-ref").await.unwrap().valid);
        assert_eq!(adapter.list_models(None).await.unwrap().len(), 1);
        assert!(adapter
            .render_codex_config("deepseek-test", None)
            .unwrap()
            .contains("base_url"));
    }
}
