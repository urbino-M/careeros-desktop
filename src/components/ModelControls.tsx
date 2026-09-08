import { t } from "../i18n";
import { useEffect, useState } from "react";
import { api } from "../api";
import type { ProviderInfo, TaskModelDefault } from "../types";

export interface ModelSelection {
  providerId: string;
  modelId: string;
  reasoning: string;
}

const builtInModelLabels = new Set([
  "Sol · 最高质量", "Terra · 平衡", "Terra · 均衡质量", "Luna · 快速", "Luna · 快速经济",
]);

export function modelDisplayLabel(providerId: string, displayName: string): string {
  return providerId === "openai" && builtInModelLabels.has(displayName) ? t(displayName) : displayName;
}

const reasoningLabels: Record<string,string> = {
  none:"不启用推理", minimal:"最少", low:"低", medium:"中", high:"高", xhigh:"XHigh · 默认高质量", max:"最高", ultra:"极致",
};

export function reasoningLabel(value: string): string { return reasoningLabels[value] ? t(reasoningLabels[value]) : value; }

export function ModelControls({
  taskType,
  value,
  onChange,
  compact = false,
}: {
  taskType: string;
  value?: ModelSelection;
  onChange: (value: ModelSelection) => void;
  compact?: boolean;
}) {
  const [providers, setProviders] = useState<ProviderInfo[]>([]);
  const [defaults, setDefaults] = useState<TaskModelDefault[]>([]);

  useEffect(() => {
    Promise.all([api.providers(), api.taskDefaults()]).then(([p, d]) => {
      setProviders(p);
      setDefaults(d);
      if (!value) {
        const fallback = d.find((item) => item.taskType === taskType) ?? d[0];
        if (fallback) onChange({ providerId: fallback.providerId, modelId: fallback.modelId, reasoning: fallback.reasoning });
      }
    });
  }, [taskType]);

  const enabledProviders = providers.filter((provider) => provider.enabled);
  const currentProvider = enabledProviders.find((provider) => provider.id === value?.providerId) ?? enabledProviders[0];
  const models = currentProvider?.models.filter((model) => model.enabled) ?? [];
  const current = value ?? (() => {
    const fallback = defaults.find((item) => item.taskType === taskType);
    return fallback ? { providerId: fallback.providerId, modelId: fallback.modelId, reasoning: fallback.reasoning } : undefined;
  })();
  const selectedModel = models.find((model) => model.id === current?.modelId);
  const reasoningOptions = selectedModel?.reasoningLevels.length
    ? selectedModel.reasoningLevels
    : ["low", "medium", "high"];

  if (!current) return <div className="field-hint">{t("正在读取模型设置…")}</div>;

  const update = (next: Partial<ModelSelection>) => onChange({ ...current, ...next });
  return (
    <div className={`model-controls ${compact ? "compact" : ""}`}>
      <label>
        <span>{t("Agent 服务")}</span>
        <select
          value={current.providerId}
          onChange={(event) => {
            const provider = enabledProviders.find((item) => item.id === event.target.value);
            const model = provider?.models.find((item) => item.enabled);
            const levels = model?.reasoningLevels || [];
            update({
              providerId: event.target.value,
              modelId: model?.id ?? "",
              reasoning: levels.includes("high") ? "high" : levels[0] || "medium",
            });
          }}
        >
          {enabledProviders.map((provider) => <option value={provider.id} key={provider.id}>{provider.displayName}</option>)}
        </select>
      </label>
      <label>
        <span>{t("模型")}</span>
        <select value={current.modelId} onChange={(event) => {
          const model = models.find((item) => item.id === event.target.value);
          const levels = model?.reasoningLevels || [];
          update({
            modelId: event.target.value,
            reasoning: levels.includes(current.reasoning)
              ? current.reasoning
              : levels.includes("high") ? "high" : levels[0] || "medium",
          });
        }}>
          {models.map((model) => <option value={model.id} key={model.id}>{modelDisplayLabel(currentProvider?.id ?? current.providerId,model.displayName)}</option>)}
        </select>
      </label>
      <label>
        <span>{t("推理强度")}</span>
        <select value={current.reasoning} onChange={(event) => update({ reasoning: event.target.value })}>
          {reasoningOptions.map((item) => <option value={item} key={item}>{reasoningLabel(item)}</option>)}
        </select>
      </label>
    </div>
  );
}
