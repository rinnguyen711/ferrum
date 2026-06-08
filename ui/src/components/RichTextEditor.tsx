import { useEditor, EditorContent } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Underline from "@tiptap/extension-underline";
import Link from "@tiptap/extension-link";
import Highlight from "@tiptap/extension-highlight";
import TextAlign from "@tiptap/extension-text-align";
import { useState, useEffect, useRef } from "react";
import { MediaImageExtension } from "./MediaImageExtension";
import { AssetPicker } from "../screens/media/AssetPicker";
import type { MediaAsset } from "../api/types";
import {
  Bold, Italic, Underline as UnderlineIcon, Strikethrough,
  Code, Code2, Highlighter, Link2, AlignLeft, AlignCenter,
  AlignRight, AlignJustify, List, ListOrdered, TextQuote,
  Image, Undo2, Redo2, Heading1, Heading2, Heading3,
} from "lucide-react";

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
  const skipNextSync = useRef(false);

  const editor = useEditor({
    extensions,
    content: toContent(value),
    editable: !disabled,
    onUpdate({ editor }) {
      skipNextSync.current = true;
      onChange(editor.getJSON());
    },
  });

  useEffect(() => {
    if (!editor) return;
    if (skipNextSync.current) { skipNextSync.current = false; return; }
    const incoming = toContent(value);
    if (incoming) editor.commands.setContent(incoming, { emitUpdate: false });
  }, [editor, value]);

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
        {btn(false, () => editor.chain().focus().undo().run(), "Undo", <Undo2 size={14} />)}
        {btn(false, () => editor.chain().focus().redo().run(), "Redo", <Redo2 size={14} />)}
        {sep()}
        {btn(editor.isActive("heading", { level: 1 }), () => editor.chain().focus().toggleHeading({ level: 1 }).run(), "H1", <Heading1 size={14} />)}
        {btn(editor.isActive("heading", { level: 2 }), () => editor.chain().focus().toggleHeading({ level: 2 }).run(), "H2", <Heading2 size={14} />)}
        {btn(editor.isActive("heading", { level: 3 }), () => editor.chain().focus().toggleHeading({ level: 3 }).run(), "H3", <Heading3 size={14} />)}
        {sep()}
        {btn(editor.isActive("bulletList"), () => editor.chain().focus().toggleBulletList().run(), "Bullet list", <List size={14} />)}
        {btn(editor.isActive("orderedList"), () => editor.chain().focus().toggleOrderedList().run(), "Ordered list", <ListOrdered size={14} />)}
        {btn(editor.isActive("blockquote"), () => editor.chain().focus().toggleBlockquote().run(), "Blockquote", <TextQuote size={14} />)}
        {sep()}
        {btn(editor.isActive("bold"), () => editor.chain().focus().toggleBold().run(), "Bold", <Bold size={14} />)}
        {btn(editor.isActive("italic"), () => editor.chain().focus().toggleItalic().run(), "Italic", <Italic size={14} />)}
        {btn(editor.isActive("underline"), () => editor.chain().focus().toggleUnderline().run(), "Underline", <UnderlineIcon size={14} />)}
        {btn(editor.isActive("strike"), () => editor.chain().focus().toggleStrike().run(), "Strikethrough", <Strikethrough size={14} />)}
        {sep()}
        {btn(editor.isActive("code"), () => editor.chain().focus().toggleCode().run(), "Inline code", <Code size={14} />)}
        {btn(editor.isActive("codeBlock"), () => editor.chain().focus().toggleCodeBlock().run(), "Code block", <Code2 size={14} />)}
        {btn(editor.isActive("highlight"), () => editor.chain().focus().toggleHighlight().run(), "Highlight", <Highlighter size={14} />)}
        {sep()}
        {btn(editor.isActive("link"), setLink, "Link", <Link2 size={14} />)}
        {sep()}
        {btn(editor.isActive({ textAlign: "left" }), () => editor.chain().focus().setTextAlign("left").run(), "Align left", <AlignLeft size={14} />)}
        {btn(editor.isActive({ textAlign: "center" }), () => editor.chain().focus().setTextAlign("center").run(), "Align center", <AlignCenter size={14} />)}
        {btn(editor.isActive({ textAlign: "right" }), () => editor.chain().focus().setTextAlign("right").run(), "Align right", <AlignRight size={14} />)}
        {btn(editor.isActive({ textAlign: "justify" }), () => editor.chain().focus().setTextAlign("justify").run(), "Justify", <AlignJustify size={14} />)}
        {sep()}
        {btn(false, () => setShowPicker(true), "Insert image", <Image size={14} />)}
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
