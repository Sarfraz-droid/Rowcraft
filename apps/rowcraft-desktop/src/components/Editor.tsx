import { autocompletion } from "@codemirror/autocomplete";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { Compartment, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { basicSetup } from "codemirror";
import { type RefObject, useEffect, useRef } from "react";

import { DEFAULT_EDITOR_PREFERENCES, type EditorPreferences } from "../preferences";
import { statementToRun } from "../grid";
import type { Catalog } from "../api";
import { editorLanguage } from "../editor-language";

export interface EditorHandle {
  text(): string;
  /** The selection if there is one, otherwise the whole text; `null` when there is nothing. */
  statement(): string | null;
  /** Replaces the whole text. */
  setText(text: string): void;
  selection?(): { from: number; to: number; text: string };
  replaceRange?(from: number, to: number, text: string): void;
  cursorCoordinates?(): { left: number; bottom: number } | null;
  focus?(): void;
}

// Colors come from shared theme tokens (styles/theme.css), so the editor follows light and dark mode.
const theme = EditorView.theme({
  "&": { height: "100%", color: "var(--text)", backgroundColor: "var(--bg)" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: "var(--mono)", fontSize: "var(--editor-font-size, 13px)", lineHeight: "1.55" },
  ".cm-content": { caretColor: "var(--text)" },
  ".cm-cursor": { borderLeftColor: "var(--text)" },
  ".cm-gutters": {
    backgroundColor: "var(--bg)",
    color: "var(--muted-foreground)",
    border: "none",
  },
  ".cm-tooltip": {
    backgroundColor: "var(--popover)", color: "var(--popover-foreground)",
    border: "1px solid var(--border)", borderRadius: "var(--radius)", boxShadow: "var(--shadow-md)",
  },
  ".cm-tooltip.cm-tooltip-autocomplete": { maxWidth: "min(480px, calc(100vw - 24px))" },
  ".cm-tooltip.cm-tooltip-autocomplete > ul": {
    fontFamily: "var(--mono)", fontSize: "12px", minWidth: "260px", maxWidth: "100%",
    maxHeight: "280px", height: "auto", padding: "4px", margin: "0",
  },
  ".cm-tooltip.cm-tooltip-autocomplete > ul > li": {
    display: "flex", alignItems: "center", gap: "8px", minHeight: "28px",
    padding: "4px 8px", lineHeight: "1.5", color: "var(--popover-foreground)",
  },
  ".cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]": {
    backgroundColor: "var(--primary)", color: "var(--primary-foreground)",
  },
  ".cm-tooltip-autocomplete .cm-completionLabel": {
    flex: "1", minWidth: "0", overflow: "hidden", textOverflow: "ellipsis",
  },
  ".cm-completionKind": {
    flexShrink: "0", width: "30px", fontSize: "9px", fontWeight: "600", opacity: ".8",
  },
  ".cm-completionDetail": {
    marginLeft: "12px", fontSize: "10px", fontStyle: "normal", color: "var(--muted-foreground)",
    maxWidth: "150px", overflow: "hidden", textOverflow: "ellipsis",
  },
  ".cm-tooltip-autocomplete li[aria-selected] .cm-completionDetail": { color: "inherit", opacity: ".85" },
  ".cm-completionMatchedText": { fontWeight: "700", textDecoration: "none" },
  ".cm-tooltip.cm-completionInfo": {
    fontFamily: "var(--font-sans)", fontSize: "11px", lineHeight: "1.6", padding: "8px 12px",
  },
  ".cm-activeLine": { backgroundColor: "var(--hover)" },
  ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--text)" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: "var(--selection) !important",
  },
});

const highlight = syntaxHighlighting(
  HighlightStyle.define([
    { tag: tags.keyword, color: "var(--syn-keyword)", fontWeight: "600" },
    { tag: [tags.string, tags.special(tags.string)], color: "var(--syn-string)" },
    { tag: [tags.number, tags.bool, tags.null], color: "var(--syn-number)" },
    { tag: [tags.lineComment, tags.blockComment], color: "var(--muted-foreground)", fontStyle: "italic" },
    { tag: [tags.typeName, tags.standard(tags.name)], color: "var(--syn-type)" },
  ]),
);

export function Editor({ initial, handle, dialect = "", catalog = null, schema, onChange, preferences = DEFAULT_EDITOR_PREFERENCES }: {
  preferences?: EditorPreferences;
  initial: string;
  handle: RefObject<EditorHandle | null>;
  dialect?: string;
  catalog?: Catalog | null;
  schema?: string;
  onChange?(sql: string): void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const activeView = useRef<EditorView | null>(null);
  const wrapping = useRef(new Compartment());
  const language = useRef<Compartment | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  useEffect(() => {
    if (!host.current) return;
    const sqlLanguage = new Compartment();
    language.current = sqlLanguage;
    const view = new EditorView({
      parent: host.current,
      state: EditorState.create({ doc: initial, extensions: [wrapping.current.of(preferences.lineWrapping ? EditorView.lineWrapping : []), basicSetup, sqlLanguage.of(editorLanguage(dialect, catalog, schema)), autocompletion({
        icons: false,
        maxRenderedOptions: 60,
        addToOptions: [{ position: 20, render(completion, _state, view) {
          const kind = view.dom.ownerDocument.createElement("span");
          kind.className = "cm-completionKind";
          kind.setAttribute("aria-hidden", "true");
          kind.textContent = ({ keyword: "SQL", type: "TBL", property: "COL", namespace: "DB", function: "FN" } as Record<string, string>)[completion.type ?? ""] ?? "SQL";
          return kind;
        } }],
      }), theme, highlight, EditorView.updateListener.of(update => {
        if (update.docChanged) onChangeRef.current?.(update.state.doc.toString());
      })] }),
    });
    activeView.current = view;
    handle.current = {
      text() { return view.state.doc.toString(); },
      statement() {
        const { from, to } = view.state.selection.main;
        return statementToRun(view.state.sliceDoc(from, to), view.state.doc.toString());
      },
      selection() {
        const { from, to } = view.state.selection.main;
        return { from, to, text: view.state.sliceDoc(from, to) };
      },
      replaceRange(from, to, text) {
        view.dispatch({ changes: { from, to, insert: text }, selection: { anchor: from + text.length } });
        view.focus();
      },
      cursorCoordinates() {
        const coordinates = view.coordsAtPos(view.state.selection.main.head);
        return coordinates && { left: coordinates.left, bottom: coordinates.bottom };
      },
      focus() { view.focus(); },
      setText(text) {
        view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
      },
    };
    return () => {
      handle.current = null;
      activeView.current = null;
      language.current = null;
      view.destroy();
    };
    // The editor is created once; `initial` is only its first content.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (activeView.current && language.current) {
      activeView.current.dispatch({ effects: language.current.reconfigure(editorLanguage(dialect, catalog, schema)) });
    }
  }, [dialect, catalog, schema]);

  useEffect(() => {
    activeView.current?.dispatch({ effects: wrapping.current.reconfigure(preferences.lineWrapping ? EditorView.lineWrapping : []) });
  }, [preferences.lineWrapping]);

  return <div className="editor tw:h-full" style={{ "--editor-font-size": `${preferences.fontSize}px` } as React.CSSProperties} ref={host} />;
}
