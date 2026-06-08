import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Icons } from "../components/icons";
import { Notice } from "../components/ui";
import { ApiError } from "../api/client";
import {
  listMediaProviders, getMediaSettings, putMediaSettings, testMediaSettings,
} from "../api/endpoints";
import type { MediaProviderDescriptor, MediaSettings as MediaSettingsT } from "../api/types";
import { ProviderForm } from "./media/ProviderForm";

const MASK = "••••";
type Status = { kind: "idle" | "testing" | "saving" | "ok" | "error"; message?: string };

export function MediaSettings() {
  const navigate = useNavigate();
  const [providers, setProviders] = useState<MediaProviderDescriptor[]>([]);
  const [stored, setStored] = useState<MediaSettingsT | null>(null);
  const [provider, setProvider] = useState<string>("local");
  const [config, setConfig] = useState<Record<string, string>>({});
  const [status, setStatus] = useState<Status>({ kind: "idle" });
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([listMediaProviders(), getMediaSettings()])
      .then(([provs, settings]) => {
        setProviders(provs);
        setStored(settings);
        const initial = settings?.provider ?? "local";
        setProvider(initial);
        setConfig(settings && settings.provider === initial ? { ...settings.config } : {});
      })
      .catch((e) => setStatus({ kind: "error", message: e instanceof Error ? e.message : "Failed to load." }))
      .finally(() => setLoading(false));
  }, []);

  const descriptor = useMemo(
    () => providers.find((p) => p.id === provider),
    [providers, provider],
  );

  const selectProvider = (id: string) => {
    setProvider(id);
    setConfig(stored && stored.provider === id ? { ...stored.config } : {});
    setFieldErrors({});
    setStatus({ kind: "idle" });
  };

  const setField = (name: string, value: string) => {
    setConfig((c) => ({ ...c, [name]: value }));
    setFieldErrors((fe) => { const next = { ...fe }; delete next[name]; return next; });
  };

  const buildBody = (): MediaSettingsT => {
    const out: Record<string, string> = {};
    for (const f of descriptor?.fields ?? []) {
      if (f.secret) {
        const v = config[f.name];
        out[f.name] = v && v !== "" ? v : MASK;
      } else {
        out[f.name] = config[f.name] ?? "";
      }
    }
    return { provider, config: out };
  };

  const validateRequired = (): boolean => {
    const errs: Record<string, string> = {};
    for (const f of descriptor?.fields ?? []) {
      if (f.required && !f.secret && !(config[f.name] ?? "").trim()) {
        errs[f.name] = "Required";
      }
    }
    setFieldErrors(errs);
    return Object.keys(errs).length === 0;
  };

  const onTest = async () => {
    setStatus({ kind: "testing" });
    try {
      await testMediaSettings(buildBody());
      setStatus({ kind: "ok", message: "Connection OK" });
    } catch (e) {
      setStatus({ kind: "error", message: e instanceof Error ? e.message : "Connection failed." });
    }
  };

  const onSave = async () => {
    if (!validateRequired()) { setStatus({ kind: "idle" }); return; }
    setStatus({ kind: "saving" });
    try {
      await putMediaSettings(buildBody());
      const fresh = await getMediaSettings();
      setStored(fresh);
      if (fresh) setConfig({ ...fresh.config });
      setStatus({ kind: "ok", message: "Settings saved" });
    } catch (e) {
      if (e instanceof ApiError && e.fieldErrors.length) {
        const errs: Record<string, string> = {};
        for (const fe of e.fieldErrors) errs[fe.field] = fe.message ?? "Invalid";
        setFieldErrors(errs);
        setStatus({ kind: "error", message: "Please fix the highlighted fields." });
      } else {
        setStatus({ kind: "error", message: e instanceof Error ? e.message : "Could not save." });
      }
    }
  };

  const busy = status.kind === "testing" || status.kind === "saving";
  const activeLabel = providers.find((p) => p.id === stored?.provider)?.label;

  return (
    <div className="rs-cm">
      <div className="rs-cm-head">
        <div>
          <h1>Media Storage</h1>
          <p className="rs-cm-sub">
            {loading ? "Loading…"
              : stored ? `Active provider: ${activeLabel ?? stored.provider}`
              : "No provider configured — defaults to local filesystem."}
          </p>
        </div>
        <button className="rs-btn rs-btn--ghost" type="button" onClick={() => navigate("/media")}>
          <Icons.arrowLeft size={16} /> Back to Media
        </button>
      </div>

      {!loading && (
        <div className="rs-settings-card">
          <div className="rs-field">
            <div className="rs-field-label"><label>Storage provider</label></div>
            <select className="rs-input" value={provider} onChange={(e) => selectProvider(e.target.value)}>
              {providers.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
            </select>
          </div>

          {descriptor && (
            <ProviderForm descriptor={descriptor} values={config} onChange={setField} fieldErrors={fieldErrors} />
          )}

          {status.kind === "ok" && <Notice tone="ok">{status.message}</Notice>}
          {status.kind === "error" && <Notice>{status.message}</Notice>}

          <div className="rs-editor-actions" style={{ marginTop: 16 }}>
            <button className="rs-btn rs-btn--ghost" type="button" disabled={busy} onClick={onTest}>
              <Icons.bolt size={15} /> {status.kind === "testing" ? "Testing…" : "Test connection"}
            </button>
            <div className="rs-spacer" />
            <button className="rs-btn rs-btn--primary" type="button" disabled={busy} onClick={onSave}>
              <Icons.check size={15} /> {status.kind === "saving" ? "Saving…" : "Save settings"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
