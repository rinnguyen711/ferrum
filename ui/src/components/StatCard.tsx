import { Icons, type IconKey } from "./icons";

/** Tinted icon + value tile used on the dashboard and audit log. */
export function StatCard({
  label, value, delta, icon, tone, mono,
}: {
  label: string;
  value: string | number;
  delta: string;
  icon: IconKey;
  tone: string;
  mono?: boolean;
}) {
  const I = Icons[icon];
  return (
    <div className={"rs-stat rs-stat--" + tone}>
      <div className="rs-stat-icon"><I size={18} /></div>
      <div className="rs-stat-body">
        <span className="rs-stat-label">{label}</span>
        <strong className={"rs-stat-value" + (mono ? " rs-mono" : "")}>{value}</strong>
        <span className="rs-stat-delta">{delta}</span>
      </div>
    </div>
  );
}
