import {
  createContext, useCallback, useContext, useEffect, useMemo, useRef, useState,
  type ReactNode,
} from "react";
import { useNavigate } from "react-router-dom";
import { ApiError } from "../api/client";
import { createContentType, patchContentType } from "../api/endpoints";
import type { ContentType, ContentTypeKind } from "../api/types";
import {
  type Draft, diffToPatch, isDirty, newDraft, seedFromContentType,
  toNewContentType,
} from "./draftModel";

interface BuilderDraftCtx {
  draft: Draft | null;
  dirty: boolean;
  saving: boolean;
  banner: string | null;
  fieldErrors: Record<string, string>;
  saveNonce: number;
  startNew(name: string, display: string, kind?: ContentTypeKind): void;
  loadExisting(ct: ContentType): void;
  setDraft(updater: (d: Draft) => Draft): void;
  clearBanner(): void;
  save(): Promise<void>;
  reset(): void;
  guardedNavigate(to: string): void;
  bumpNonce(): void;
}

const Ctx = createContext<BuilderDraftCtx | null>(null);

export function useBuilderDraft(): BuilderDraftCtx {
  const v = useContext(Ctx);
  if (!v) throw new Error("useBuilderDraft outside BuilderDraftProvider");
  return v;
}

export function BuilderDraftProvider({ children }: { children: ReactNode }) {
  const navigate = useNavigate();
  const [draft, setDraftState] = useState<Draft | null>(null);
  const [saving, setSaving] = useState(false);
  const [banner, setBanner] = useState<string | null>(null);
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});
  const [saveNonce, setSaveNonce] = useState(0);

  const dirty = isDirty(draft);

  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;

  useEffect(() => {
    const handler = (e: BeforeUnloadEvent) => {
      if (dirtyRef.current) {
        e.preventDefault();
        e.returnValue = "";
      }
    };
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, []);

  const startNew = useCallback((name: string, display: string, kind: ContentTypeKind = "collection") => {
    setBanner(null);
    setFieldErrors({});
    setDraftState(newDraft(name, display, kind));
  }, []);

  const loadExisting = useCallback((ct: ContentType) => {
    setBanner(null);
    setFieldErrors({});
    setDraftState(seedFromContentType(ct));
  }, []);

  const setDraft = useCallback((updater: (d: Draft) => Draft) => {
    setDraftState((d) => (d ? updater(d) : d));
  }, []);

  const reset = useCallback(() => {
    setDraftState(null);
    setBanner(null);
    setFieldErrors({});
  }, []);

  const clearBanner = useCallback(() => setBanner(null), []);

  const applyApiError = useCallback((e: unknown, fallback: string) => {
    if (e instanceof ApiError) {
      if (e.fieldErrors.length) {
        const map: Record<string, string> = {};
        for (const fe of e.fieldErrors) map[fe.field] = fe.message ?? "Invalid";
        setFieldErrors(map);
      }
      setBanner(e.message);
    } else {
      setBanner(fallback);
    }
  }, []);

  const save = useCallback(async () => {
    if (!draft) return;
    setBanner(null);
    setFieldErrors({});

    if (draft.mode === "new") {
      if (draft.fields.length === 0) {
        setBanner("Add at least one field before saving.");
        return;
      }
      setSaving(true);
      try {
        const ct = await createContentType(toNewContentType(draft));
        setDraftState(seedFromContentType(ct));
        setSaveNonce((n) => n + 1);
        navigate(`/builder/${ct.name}`);
      } catch (e) {
        applyApiError(e, "Create failed.");
      } finally {
        setSaving(false);
      }
      return;
    }

    const patch = diffToPatch(draft);
    setSaving(true);
    try {
      const ct = await patchContentType(draft.name, patch);
      setDraftState(seedFromContentType(ct));
      setSaveNonce((n) => n + 1);
    } catch (e) {
      applyApiError(e, "Save failed.");
    } finally {
      setSaving(false);
    }
  }, [draft, navigate, applyApiError]);

  const guardedNavigate = useCallback(
    (to: string) => {
      if (dirtyRef.current) {
        const ok = window.confirm("You have unsaved changes. Leave anyway?");
        if (!ok) return;
        reset();
      }
      navigate(to);
    },
    [navigate, reset],
  );

  const bumpNonce = useCallback(() => setSaveNonce((n) => n + 1), []);

  const value = useMemo<BuilderDraftCtx>(
    () => ({
      draft, dirty, saving, banner, fieldErrors, saveNonce,
      startNew, loadExisting, setDraft, clearBanner, save, reset, guardedNavigate, bumpNonce,
    }),
    [draft, dirty, saving, banner, fieldErrors, saveNonce, startNew, loadExisting,
     setDraft, clearBanner, save, reset, guardedNavigate, bumpNonce],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
