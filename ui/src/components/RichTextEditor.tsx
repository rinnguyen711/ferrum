import { useEditor, EditorContent } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Underline from "@tiptap/extension-underline";
import Link from "@tiptap/extension-link";
import Highlight from "@tiptap/extension-highlight";
import TextAlign from "@tiptap/extension-text-align";
import { useState } from "react";
import { MediaImageExtension } from "./MediaImageExtension";
import { AssetPicker } from "../screens/media/AssetPicker";
import type { MediaAsset } from "../api/types";

const extensions = [
  StarterKit,
  Underline,
  Link.configure({ openOnClick: false }),
  Highlight,
  TextAlign.configure({ types: ["heading", "paragraph"] }),
  MediaImageExtension,
];

function toContent(value: unknown): object | undefined {
  if (value && typeof value === "object" && !Array.isArray(value)) {
    return value as object;
  }
  return undefined;
}

export function RichTextEditor({
  value,
  onChange,
  disabled,
}: {
  value: unknown;
  onChange: (v: unknown) => void;
  disabled?: boolean;
}) {
  const [showPicker, setShowPicker] = useState(false);

  const editor = useEditor({
    extensions,
    content: toContent(value),
    editable: !disabled,
    onUpdate({ editor }) {
      onChange(editor.getJSON());
    },
  });

  if (!editor) return null;

  const btn = (active: boolean, onClick: () => void, title: string, children: React.ReactNode) => (
    <button
      type="button"
      className={"rs-btn ghost rs-toolbar-btn" + (active ? " is-active" : "")}
      onMouseDown={(e) => { e.preventDefault(); onClick(); }}
      title={title}
    >
      {children}
    </button>
  );

  const sep = () => <span className="rs-toolbar-sep" />;

  const setLink = () => {
    const prev = editor.getAttributes("link").href ?? "";
    const url = window.prompt("URL", prev);
    if (url === null) return;
    if (url === "") { editor.chain().focus().unsetLink().run(); return; }
    editor.chain().focus().setLink({ href: url }).run();
  };

  const handleImagePick = (assets: MediaAsset[]) => {
    setShowPicker(false);
    const asset = assets[0];
    if (!asset) return;
    const src = `/admin/media/assets/${asset.id}/raw`;
    (editor.chain().focus() as any).insertImage(src, asset.alt_text ?? asset.original_filename).run();
  };

  return (
    <div className="rs-rich-text">
      <div className="rs-rich-text-toolbar">
        {btn(false, () => editor.chain().focus().undo().run(), "Undo", "↩")}
        {btn(false, () => editor.chain().focus().redo().run(), "Redo", "↪")}
        {sep()}
        {btn(editor.isActive("heading", { level: 1 }), () => editor.chain().focus().toggleHeading({ level: 1 }).run(), "H1", "H1")}
        {btn(editor.isActive("heading", { level: 2 }), () => editor.chain().focus().toggleHeading({ level: 2 }).run(), "H2", "H2")}
        {btn(editor.isActive("heading", { level: 3 }), () => editor.chain().focus().toggleHeading({ level: 3 }).run(), "H3", "H3")}
        {sep()}
        {btn(editor.isActive("bulletList"), () => editor.chain().focus().toggleBulletList().run(), "Bullet list", "•")}
        {btn(editor.isActive("orderedList"), () => editor.chain().focus().toggleOrderedList().run(), "Ordered list", "1.")}
        {btn(editor.isActive("blockquote"), () => editor.chain().focus().toggleBlockquote().run(), "Blockquote", "“")}
        {sep()}
        {btn(editor.isActive("bold"), () => editor.chain().focus().toggleBold().run(), "Bold", "B")}
        {btn(editor.isActive("italic"), () => editor.chain().focus().toggleItalic().run(), "Italic", "I")}
        {btn(editor.isActive("underline"), () => editor.chain().focus().toggleUnderline().run(), "Underline", "U")}
        {btn(editor.isActive("strike"), () => editor.chain().focus().toggleStrike().run(), "Strikethrough", "S̶")}
        {sep()}
        {btn(editor.isActive("code"), () => editor.chain().focus().toggleCode().run(), "Inline code", "`")}
        {btn(editor.isActive("codeBlock"), () => editor.chain().focus().toggleCodeBlock().run(), "Code block", "```")}
        {btn(editor.isActive("highlight"), () => editor.chain().focus().toggleHighlight().run(), "Highlight", "▌")}
        {sep()}
        {btn(editor.isActive("link"), setLink, "Link", "🔗")}
        {sep()}
        {btn(editor.isActive({ textAlign: "left" }), () => editor.chain().focus().setTextAlign("left").run(), "Align left", "⬅")}
        {btn(editor.isActive({ textAlign: "center" }), () => editor.chain().focus().setTextAlign("center").run(), "Align center", "≡")}
        {btn(editor.isActive({ textAlign: "right" }), () => editor.chain().focus().setTextAlign("right").run(), "Align right", "➡")}
        {btn(editor.isActive({ textAlign: "justify" }), () => editor.chain().focus().setTextAlign("justify").run(), "Justify", "☰")}
        {sep()}
        {btn(false, () => setShowPicker(true), "Insert image", "🖼")}
      </div>
      <EditorContent editor={editor} className="rs-rich-text-content" />
      {showPicker && (
        <AssetPicker
          multiple={false}
          onPick={handleImagePick}
          onClose={() => setShowPicker(false)}
        />
      )}
    </div>
  );
}
