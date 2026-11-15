# Plan · comments-as-markdown

- **Ticket:** none
- **Date:** 2026-11-15
- **Decisions:** 110, 111, 112
- **Tech spec:** none (planned straight from the decisions and the code, 2026-11-15)
- **Orchestrator prompt:** tuq/prompts/comments-as-markdown-orchestrator.md

<!-- tuq:plan:start -->

Ticket comments become Markdown in eight pieces. One shared renderer draws a comment's Markdown (110) and every place
that shows a comment uses it. The user's comments lose their length limit, while agents keep 2,000 characters (112).
The comment box gets GitHub's helpers but stays a text box (111). Prime and one-line previews follow the same Markdown
rules. Comments are stored as they are today: no migration, and old comments are drawn by the same renderer (110).

Three rules hold in every piece. A comment's text is stored and posted exactly as written. HTML is never interpreted.
The description editor's behaviour, including how it saves Markdown, stays byte for byte the same; the round-trip
tests prove it.

## Pieces

1. A comment's Markdown is drawn by one shared renderer — waits for nothing
   A new read-only component in `crates/otaq-desktop/ui/src/markdown/` (for example `CommentMarkdown`, exported from
   `markdown/index.ts`) turns a comment's Markdown into static HTML, with no Tiptap editor per comment. It parses with
   the description's own schema (`markdownExtensions()` in `markdown/extensions.ts`, `loadMarkdown` /
   `wrapLooseInline` in `markdown/changes.ts`) and serialises the result to the DOM once. Comments and descriptions
   therefore draw the same nodes, and a thread of 50 comments doesn't start 50 editors. Only comments differ from
   descriptions, in these ways (110):
   - **Line breaks.** A single line break is a hard break (GitHub's `breaks`), except inside code blocks. A blank
     line starts a new paragraph.
   - **HTML.** Anything that looks like a tag is text: `Observable<number | null>`, `<why>`, `NN-<page>-<step>.png`,
     a pasted `<div>`. Neither `@tiptap/markdown`'s HTML tokens nor the schema may let it through as markup.
   - **Links.** Only `https:` and `http:` links are links (`LINK_SCHEMES` in `markdown/link/url.ts`); any other
     scheme, `javascript:` included, shows as text. A click opens the system browser the way a description link
     does.
   - **Images.** `![alt](src)` loads nothing and shows its alt text (or its URL when there is no alt text).
   - **Task lists.** Their boxes show but can't be ticked.
   - **Headings.** Drawn small, about the size of bold text, in the component's own CSS. `markdown.css` keeps the
     description's sizes.
   - **Code and tables.** Code blocks are highlighted (`lowlight`, as in descriptions) and scroll sideways. Tables
     scroll sideways.

   Vitest covers each rule above, plus sample texts shaped like the real comments 110 lists (placeholders in `<…>`,
   glob stars, "one line per criterion" lists, `**bold**` and backtick heavy hand-overs), plus an empty body. The
   description's round-trip and editor tests must pass unchanged.
   Check: the vitest file for the component; `markdown/roundtrip` and `Editor.test.ts` unchanged and passing.
2. Every place a comment shows uses the renderer, and long comments fold — waits for 1
   Replace the plain text with the piece 1 component in these places:
   - the Ticket tab's thread and the Delete dialog's quoted comment (`screens/editor/CommentThread.tsx`, the two
     `<p className="comment-body">`);
   - comments on a diff (`screens/files/Hunks.tsx`, `.diff-comment-body`);
   - the comments listed in the review dialogs (`screens/review/Dialogs.tsx`, `review-comment`).

   Search for other places that draw a comment's `body` (send-back and drag-back dialogs that quote a comment, a PR
   dialog) and switch them too; leave a field that isn't a comment alone.
   In the thread, a comment taller than about 15 lines shows those lines and then "Show more", which opens the rest
   in place, with "Show less" after it. The words come from Rust (`CommentThreadView`, `otaq-view`). Copy still copies
   `row.body` exactly, and Edit opens it exactly. Drop `white-space: pre-wrap` from `.comment-body` and
   `.diff-comment-body`, since line breaks now come from piece 1.
   Check: page tests for the thread (a Markdown comment drawn, Show more/less on a long one, Copy gives the raw text)
   and for a diff comment; the existing comment and review tests pass.
3. Prime nests a comment's headings under the comment — waits for nothing
   In `render_comments` (`crates/otaq-core/src/prime.rs`), every comment sits under `### <heading>`. A Markdown
   heading line inside a comment's body is pushed down so it sits below that (`#` becomes `####`, capped at six).
   Lines inside fenced code blocks are left alone. Nothing else in the body changes. Apply the same rule wherever
   prime prints a comment's body (for example a parent's "last comment" from an Agent stage).
   Check: prime tests for a comment with `#` and `##` lines, one with a `#` line inside a code fence, and a comment
   with no headings printed exactly as before.
4. One-line previews show a comment's words without Markdown marks — waits for nothing
   History's "commented" line (`excerpt` in `crates/otaq-core/src/board/ticket_comments.rs`), notices and any other
   single-line preview of a comment show its words without the marks: `**Done**` becomes "Done", `` `x` `` becomes
   "x", `[text](url)` becomes "text", and a leading `#`, `-`, `1.`, `>` or `- [ ]` is dropped. Lines are joined with
   spaces. Strip first, then cut to `EXCERPT_CHARS`, so no half-mark is left at the cut. Use one function in
   `otaq-core`, shared by every preview. A full comment (thread, prime, `otaq comments`) is never stripped.
   Check: unit tests for the stripping and the cut; the History tests updated to expect the stripped text.
5. The user's comments have no length limit; agents keep 2,000 — waits for nothing
   Which limit applies depends on who writes the comment. Today, `check_comment` (`board/ticket_comments.rs`) applies
   `COMMENT_LIMIT_CHARS` to every author. Its callers are `post_ticket_comment`, `edit_ticket_comment_with`,
   `hand_over` (`board/hand_over.rs`) and the first comment of a new ticket (`board/tickets.rs`, about line 709). Give
   it the author (`CommentAuthor`): a user's comment has no character limit, while an agent's keeps the limit and
   today's refusal text pointing to `--report-file`. Editing follows the author of the comment being edited.
   `check_body` (helper notes, drop reasons, summaries: always agents) and manual checks (`MANUAL_CHECK_LIMIT_CHARS`)
   keep 2,000. Comments on a diff (`board/comments.rs`) keep whatever limit they have today.

   `CommentLimits` (`otaq-view/src/view_models/comment_limits.rs`) tells the user's boxes that there is no
   character limit; regenerate the TypeScript type.

   The one ceiling left is what one service message can carry (`MAX_FRAME_BYTES`, 16 MB, `protocol.rs`). The box
   checks text that would not fit before sending and says so in words. The service never answers with a raw frame
   error.
   Check: core tests for a user's 48,210-character comment posted, edited and handed over; an agent's refused with
   today's text; a helper note and a manual check still refused over 2,000; an agent's `otaq comment` over the limit
   still refused (CLI test).
6. The comment box never blocks the user on length — waits for 5
   In `screens/editor/commentRules.ts` and `CommentBox.tsx`, the user's boxes drop the counter ("2,140 / 2,000"),
   the red box and the length check on the button. A paste is always inserted as text: `pasteAction` no longer turns
   long text into a "Pasted text" chip. Images and files that are pasted, dropped or attached become attachments as
   before (003). This holds in every comment box: the thread, editing a posted comment, and the Send back, Fail and
   drag-back dialogs (`board/DragBackDialog.tsx`, `review/Dialogs.tsx`, `sheet/actions.tsx`, `pr/Dialogs.tsx`).
   A "Pasted text" chip already on a posted comment still opens. Update the file header comments that describe the old
   rule.
   Check: commentRules unit tests (no counter, can submit at any length, paste always text); a page test that pastes
   50,000 characters into the thread's box and posts them; the attachment paste and drop tests pass unchanged.
7. The comment box gets Write/Preview, a toolbar and shortcuts — waits for 1, 6
   The comment box stays a `<textarea>` (111). Above it go two tabs, **Write** and **Preview**. Preview draws the
   text with the piece 1 component, and Write brings the textarea back with its caret and selection where they were.
   A toolbar over the text has bold, italic, code, link, bulleted list, numbered list, task list and quote. Each
   button wraps the selection in its marks, or with nothing selected inserts the marks with the caret between them.
   Lists and quote act on the selected lines.
   Shortcuts: ⌘B, ⌘I, ⌘E (code) and ⌘K (link), Ctrl on Windows and Linux. While the comment box has the focus, these
   keys belong to the box, the way the description editor already keeps ⌘K and ⌘B (`app/keys.ts`, `EDITOR_KEYS` and
   `keysKeptBy`; `app/registry/shortcuts.ts`). Everywhere else ⌘K stays the command palette.
   Every edit goes through the textarea's own undo, so ⌘Z undoes a toolbar click.
   The same box in every place listed in piece 6. Paste, drop, Attach, chips, ⌘↵ and Tell now work as today.
   Check: unit tests for each wrap and unwrap on a selection and at a caret; page tests for the tabs, the toolbar,
   each shortcut, ⌘K opening the palette outside the box and the link inside it, ⌘Z after a button; run the shortcut
   tests in the Windows VM (Ctrl, and decision 048's Ctrl+V).
8. Lists continue on Enter, and a URL pasted over selected text makes a link — waits for 7
   In the comment box's textarea, Enter on a line that starts `- `, `* `, `1. ` or `- [ ] ` starts the next item:
   the next number, and an unticked box for a task list. Enter on an item with no text removes its marker and ends
   the list. Shift+Enter and ⌘↵ keep their meaning.
   Pasting a single `https:`/`http:` URL while text is selected gives `[selection](url)`. Any other paste behaves as
   piece 6 left it. All of it goes through the textarea's undo.
   Check: unit tests for each list kind, the empty item and the numbered count; page tests for the link paste and for
   an ordinary paste with a selection; the paste tests of piece 6 pass unchanged.

<!-- tuq:plan:end -->
