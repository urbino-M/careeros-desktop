import { useEffect, useState } from "react";
import { api } from "../api";
import type { ProviderInfo, TaskModelDefault } from "../types";

export interface ModelSelection {
  providerId: string;
  modelId: string;
  reasoning: string;
}

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

  if (!current) return <div className="field-hint">正在读取模型设置…</div>;

  const update = (next: Partial<ModelSelection>) => onChange({ ...current, ...next });
  return (
    <div className={`model-controls ${compact ? "compact" : ""}`}>
      <label>
        <span>Agent 服务</span>
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
        <span>模型</span>
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
          {models.map((model) => <option value={model.id} key={model.id}>{model.displayName}</option>)}
        </select>
      </label>
      <label>
        <span>推理强度</span>
        <select value={current.reasoning} onChange={(event) => update({ reasoning: event.target.value })}>
          {reasoningOptions.map((item) => <option value={item} key={item}>{item === "xhigh" ? "XHigh · 默认高质量" : item[0].toUpperCase() + item.slice(1)}</option>)}
        </select>
      </label>
    </div>
  );
}
