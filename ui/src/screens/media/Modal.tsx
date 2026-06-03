import { ReactNode } from "react";
import { Icons } from "../../components/icons";

type IconName = keyof typeof Icons;

export function Modal({
  eyebrow, title, icon, wide, footer, onClose, children,
}: {
  eyebrow?: string;
  title: string;
  icon?: IconName;
  wide?: boolean;
  footer?: ReactNode;
  onClose: () => void;
  children: ReactNode;
}) {
  const IconCmp = icon ? Icons[icon] : null;
  return (
    <div className="rs-modal-backdrop" onClick={onClose}>
      <div
        className={"rs-modal" + (wide ? " rs-modal--wide" : "")}
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => { if (e.key === "Escape") onClose(); }}
      >
        <div className="rs-modal-head">
          {IconCmp && <span className="rs-modal-ico"><IconCmp size={18} /></span>}
          <div>
            {eyebrow && <span className="rs-modal-eyebrow">{eyebrow}</span>}
            <h2>{title}</h2>
          </div>
          <button className="rs-modal-x" onClick={onClose} aria-label="Close" type="button">
            <Icons.x size={18} />
          </button>
        </div>
        <div className="rs-modal-body">{children}</div>
        {footer && <div className="rs-modal-foot">{footer}</div>}
      </div>
    </div>
  );
}

export function ModalTabs({
  tab, setTab, tabs,
}: {
  tab: string;
  setTab: (t: string) => void;
  tabs: [string, string][];
}) {
  return (
    <div className="rs-modal-tabs">
      {tabs.map(([key, label]) => (
        <button
          key={key}
          type="button"
          className={"rs-modal-tab" + (tab === key ? " is-on" : "")}
          onClick={() => setTab(key)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
