import {
  createContext, useCallback, useContext, useEffect, useMemo, useRef, useState,
  type ReactNode,
} from "react";
import { useNavigate } from "react-router-dom";
import { ApiError } from "../api/client";
import { createContentType, patchContentType, updateComponent } from "../api/endpoints";
import type { Component, ContentType, ContentTypeKind } from "../api/types";
import {
  type BuilderDraft, componentToUpdate, diffToPatch, isDirty, newDraft,
  seedFromComponent, seedFromContentType, toNewContentType,
} from "./draftModel";

interface BuilderDraftCtx {
  draft: BuilderDraft | null;
  dirty: boolean;
  saving: boolean;
  banner: string | null;
  fieldErrors: Record<string, string>;
  saveNonce: number;
  startNew(name: string, display: string, kind?: ContentTypeKind): void;
  loadExisting(ct: ContentType): void;
  loadExistingComponent(c: Component): void;
  setDraft(updater: (d: BuilderDraft) => BuilderDraft): void;
  clearBanner(): void;
  save(): Promise<void>;
  /** Revert existing-type / component drafts to their server snapshot. */
  discard(): void;
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
  const [draft, setDraftState] = useState<BuilderDraft | null>(null);
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

  const loadExistingComponent = useCallback((c: Component) => {
    setBanner(null);
    setFieldErrors({});
    setDraftState(seedFromComponent(c));
  }, []);

  const setDraft = useCallback((updater: (d: BuilderDraft) => BuilderDraft) => {
    setDraftState((d) => (d ? updater(d) : d));
  }, []);

  const discard = useCallback(() => {
    setBanner(null);
    setFieldErrors({});
    setDraftState((d) => {
      if (!d) return d;
      if (d.mode === "component") return seedFromComponent(d.serverSnapshot);
      if (d.mode === "existing" && d.serverSnapshot) return seedFromContentType(d.serverSnapshot);
      return d; // mode "new" — caller decides (reset + navigate)
    });
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

    if (draft.mode === "component") {
      setSaving(true);
      try {
        const c = await updateComponent(draft.uid, componentToUpdate(draft));
        setDraftState(seedFromComponent(c));
        setSaveNonce((n) => n + 1);
      } catch (e) {
        applyApiError(e, "Save failed.");
      } finally {
        setSaving(false);
      }
      return;
    }

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
      startNew, loadExisting, loadExistingComponent, setDraft, clearBanner,
      save, discard, reset, guardedNavigate, bumpNonce,
    }),
    [draft, dirty, saving, banner, fieldErrors, saveNonce, startNew, loadExisting,
     loadExistingComponent, setDraft, clearBanner, save, discard, reset,
     guardedNavigate, bumpNonce],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
