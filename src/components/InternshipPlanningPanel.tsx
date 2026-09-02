import {
  ArrowRight,
  CheckCircle2,
  Clock3,
  GitBranch,
  MapPin,
  Radar,
  ShieldCheck,
  Sparkles,
  Target,
  Wifi,
} from "lucide-react";
import type { AppRoute } from "../types";

const searchTracks = [
  "Inference / Serving",
  "Distributed Training",
  "GPU / CUDA",
  "ML Platform / MLOps",
  "Compiler / Runtime",
  "Data Infrastructure",
  "Reliability / Observability",
];

const contributions = [
  { id: "#675", label: "Rust · Python · SQLite memory indexing" },
  { id: "#677", label: "Agent tool input normalization" },
  { id: "#678", label: "Remote API · WebSocket auth" },
  { id: "#674", label: "Cross-platform CLI reliability" },
];

const improvementTracks = [
  { index: "01", title: "Serving / Benchmark", state: "下一步", detail: "补充 latency、throughput 和 memory 的可复现实验。" },
  { index: "02", title: "GPU / CUDA", state: "待补证", detail: "用 profiling 或 kernel 优化建立第一条 GPU 证据。" },
  { index: "03", title: "Distributed Systems", state: "待补证", detail: "围绕故障恢复、队列或多机推理做一个可合并贡献。" },
];

export function InternshipPlanningSummary({ onNavigate }: { onNavigate: (route: AppRoute) => void }) {
  return (
    <section className="internship-context-strip" aria-label="当前 Internship 求职主线">
      <div className="internship-context-title">
        <Radar size={19} />
        <div><span>当前求职主线</span><strong>AI Infra</strong></div>
      </div>
      <div className="internship-context-facts">
        <span><MapPin size={14} /> 香港现场</span>
        <span><Wifi size={14} /> Remote from Hong Kong</span>
        <span><Clock3 size={14} /> 时间与时长不限</span>
      </div>
      <button className="text-button" onClick={() => onNavigate({ page: "applications", careerSystem: "internship", status: "all", view: "strategy" })}>
        查看求职策略 <ArrowRight size={15} />
      </button>
    </section>
  );
}

export function InternshipPlanningPanel({ onNavigate }: { onNavigate: (route: AppRoute) => void }) {
  return (
    <section className="internship-planning" aria-label="Internship 求职策略">
      <header className="internship-planning-header">
        <div>
          <div className="eyebrow">CURRENT TRACK · AI INFRA</div>
          <h2>求职策略</h2>
          <p>先用多来源检索保证机会覆盖，再把岗位要求映射到你的主简历和下一项工程贡献。</p>
        </div>
        <div className="internship-constraints">
          <span><MapPin size={15} /> 香港现场</span>
          <span><Wifi size={15} /> Remote from Hong Kong</span>
          <span><Clock3 size={15} /> 时间与时长不限</span>
        </div>
      </header>

      <div className="internship-planning-grid">
        <article className="planning-card planning-radar-card">
          <div className="planning-card-heading">
            <div className="planning-icon"><Radar size={21} /></div>
            <div><span className="planning-index">01 · 机会雷达</span><h3>全面覆盖 AI Infra</h3></div>
          </div>
          <div className="planning-summary"><span>检索配置</span><strong>7 个方向 · 4 类来源</strong></div>
          <div className="planning-source-row"><span>官方职位页</span><span>Greenhouse / Lever</span><span>香港岗位</span><span>Remote from HK</span></div>
          <div className="planning-track-list">
            {searchTracks.map((track, index) => <span key={track} className={index < 3 ? "priority" : ""}>{track}</span>)}
          </div>
          <button className="button primary wide" onClick={() => onNavigate({ page: "automation" })}><Radar size={16} /> 在 Agent 中启动全面扫描 <ArrowRight size={16} /></button>
        </article>

        <article className="planning-card planning-resume-card">
          <div className="planning-card-heading">
            <div className="planning-icon"><GitBranch size={21} /></div>
            <div><span className="planning-index">02 · 主简历证据</span><h3>一份母版，持续积累</h3></div>
          </div>
          <div className="evidence-count"><strong>4</strong><span>条已核验的 OpenJarvis merged PR</span></div>
          <div className="evidence-list">
            {contributions.map((item) => <div className="evidence-row" key={item.id}><CheckCircle2 size={15} /><strong>{item.id}</strong><span>{item.label}</span></div>)}
          </div>
          <div className="planning-note"><ShieldCheck size={16} /> 只把你确认的贡献写入主简历；不会把 fork 的全部代码当成个人经历。</div>
        </article>

        <article className="planning-card planning-improvement-card">
          <div className="planning-card-heading">
            <div className="planning-icon"><Target size={21} /></div>
            <div><span className="planning-index">03 · 岗位针对性提升</span><h3>把缺口变成贡献</h3></div>
          </div>
          <div className="improvement-list">
            {improvementTracks.map((item) => <div className="improvement-row" key={item.index}>
              <span className="improvement-index">{item.index}</span>
              <div><div className="improvement-title"><strong>{item.title}</strong><span>{item.state}</span></div><p>{item.detail}</p></div>
            </div>)}
          </div>
          <button className="text-button planning-action" onClick={() => onNavigate({ page: "automation" })}><Sparkles size={15} /> 让 Agent 拆解下一步 <ArrowRight size={15} /></button>
        </article>
      </div>
    </section>
  );
}
