import { useEffect, useState } from "react";
import { fetchAssetBlob } from "../../api/endpoints";
import type { MediaAsset } from "../../api/types";

/** Stable gradient from the asset id (fallback for non-images / load errors). */
function coverBg(seed: string): string {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) % 360;
  return `linear-gradient(135deg, hsl(${h} 50% 80%), hsl(${(h + 18) % 360} 45% 62%))`;
}

function extOf(a: MediaAsset): string {
  const m = a.original_filename.match(/\.([a-z0-9]+)$/i);
  if (m) return m[1].toUpperCase();
  const sub = a.mime_type.split("/")[1];
  return (sub || "file").toUpperCase();
}

export function AssetThumb({ asset, className }: { asset: MediaAsset; className?: string }) {
  const isImage = asset.mime_type.startsWith("image/");
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (!isImage) return;
    let revoked = false;
    let objectUrl: string | null = null;
    fetchAssetBlob(asset.id)
      .then((blob) => {
        if (revoked) return;
        objectUrl = URL.createObjectURL(blob);
        setUrl(objectUrl);
      })
      .catch(() => setFailed(true));
    return () => {
      revoked = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [asset.id, isImage]);

  const cls = "rs-media-cover" + (className ? " " + className : "");
  if (isImage && url && !failed) {
    return (
      <div className={cls} style={{ backgroundImage: `url(${url})`, backgroundSize: "cover", backgroundPosition: "center" }}>
        <span className="rs-media-ext rs-mono">{extOf(asset)}</span>
      </div>
    );
  }
  return (
    <div className={cls} style={{ background: coverBg(asset.id) }}>
      <span className="rs-media-ext rs-mono">{extOf(asset)}</span>
    </div>
  );
}
