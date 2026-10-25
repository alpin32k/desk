// The side panel's text editor: CodeMirror with syntax colors taken from the
// desk theme (its terminal palette, as CSS variables set by applyTheme).

import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import {
  bracketMatching,
  foldGutter,
  HighlightStyle,
  indentOnInput,
  LanguageDescription,
  syntaxHighlighting,
} from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState } from "@codemirror/state";
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import { tags as t } from "@lezer/highlight";

/** ANSI color `n` of the active theme. */
const ansi = (n: number) => `var(--ansi-${n})`;

const colors = HighlightStyle.define([
  { tag: [t.keyword, t.operatorKeyword, t.modifier, t.controlKeyword], color: ansi(5) },
  { tag: [t.string, t.special(t.string), t.regexp, t.inserted], color: ansi(2) },
  { tag: [t.number, t.bool, t.null, t.atom], color: ansi(3) },
  { tag: [t.comment, t.meta], color: "var(--dim)", fontStyle: "italic" },
  { tag: [t.function(t.variableName), t.function(t.propertyName), t.macroName], color: ansi(4) },
  { tag: [t.typeName, t.className, t.namespace, t.tagName], color: ansi(6) },
  { tag: [t.propertyName, t.attributeName, t.labelName], color: ansi(12) },
  { tag: [t.deleted, t.invalid], color: ansi(1) },
  { tag: t.heading, color: ansi(4), fontWeight: "bold" },
  { tag: [t.link, t.url], color: ansi(6), textDecoration: "underline" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strong, fontWeight: "bold" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: [t.processingInstruction, t.escape], color: ansi(13) },
]);

const look = EditorView.theme({
  "&": { height: "100%", color: "var(--fg)", backgroundColor: "var(--bg)", fontSize: "12px" },
  ".cm-scroller": { fontFamily: "var(--mono)", lineHeight: "1.55" },
  ".cm-content": { caretColor: "var(--fg)", padding: "8px 0" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--fg)" },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: "color-mix(in srgb, var(--accent) 30%, var(--bg))",
  },
  ".cm-gutters": { backgroundColor: "var(--panel)", color: "var(--dim)", border: "none" },
  ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--fg) 5%, transparent)" },
  ".cm-activeLineGutter": { backgroundColor: "color-mix(in srgb, var(--fg) 8%, transparent)", color: "var(--fg)" },
  ".cm-matchingBracket": { backgroundColor: "color-mix(in srgb, var(--accent) 25%, transparent)", outline: "none" },
  ".cm-selectionMatch": { backgroundColor: "color-mix(in srgb, var(--waiting) 18%, transparent)" },
  ".cm-searchMatch": { backgroundColor: "color-mix(in srgb, var(--waiting) 30%, transparent)" },
  ".cm-panels": { backgroundColor: "var(--panel)", color: "var(--fg)" },
  ".cm-panels input, .cm-panels button": { font: "inherit", color: "var(--fg)" },
});

export interface Editor {
  readonly view: EditorView;
  value(): string;
  focus(): void;
  destroy(): void;
}

/** An editor for `text` in `parent`, colored by the language `fileName` suggests. */
export function createEditor(
  parent: HTMLElement,
  text: string,
  fileName: string,
  on: { change(): void; save(): void },
): Editor {
  const language = new Compartment();
  const view = new EditorView({
    parent,
    state: EditorState.create({
      doc: text,
      extensions: [
        lineNumbers(),
        foldGutter(),
        highlightActiveLineGutter(),
        highlightSpecialChars(),
        history(),
        drawSelection(),
        indentOnInput(),
        bracketMatching(),
        highlightActiveLine(),
        highlightSelectionMatches(),
        search({ top: true }),
        EditorState.tabSize.of(2),
        // Prose wraps; code keeps its lines (and scrolls sideways).
        /\.(md|markdown|mdx|txt|rst|adoc)$/i.test(fileName) ? EditorView.lineWrapping : [],
        language.of([]),
        syntaxHighlighting(colors),
        look,
        keymap.of([
          { key: "Mod-s", preventDefault: true, run: () => (on.save(), true) },
          indentWithTab,
          ...defaultKeymap,
          ...historyKeymap,
          ...searchKeymap,
        ]),
        EditorView.updateListener.of((u) => u.docChanged && on.change()),
      ],
    }),
  });
  // Languages load on demand (one chunk each), so the editor shows at once.
  const desc = LanguageDescription.matchFilename(languages, fileName);
  void desc?.load().then((lang) => view.dispatch({ effects: language.reconfigure(lang) }));
  return {
    view,
    value: () => view.state.doc.toString(),
    focus: () => view.focus(),
    destroy: () => view.destroy(),
  };
}
