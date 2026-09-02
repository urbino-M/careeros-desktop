export const UPDATE_CHECK_INTERVAL_MS = 60 * 60 * 1000;
export const UPDATE_CHECK_TIMEOUT_MS = 15_000;

export function updateProgress(downloaded: number, total?: number) {
  if (!total || total <= 0) return undefined;
  return Math.min(100, Math.round((downloaded / total) * 100));
}

export function updateErrorMessage(value: unknown) {
  const raw = value instanceof Error ? value.message : String(value);
  const normalized = raw.toLowerCase();
  if (normalized.includes("signature") || normalized.includes("public key")) {
    return "更新包签名校验失败，已停止安装。请等待发布方修复。";
  }
  if (normalized.includes("timed out") || normalized.includes("timeout")) {
    return "检查更新超时，请检查网络后重试。";
  }
  if (normalized.includes("network") || normalized.includes("dns") || normalized.includes("connect")) {
    return "暂时无法连接更新服务，请检查网络后重试。";
  }
  return "更新没有完成，请稍后重试或前往 GitHub Release 手动下载。";
}

export function formatLastChecked(value?: string) {
  if (!value) return "尚未检查";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "尚未检查" : date.toLocaleString();
}
