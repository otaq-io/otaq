#!/bin/zsh
# seed.sh: create the demo repository (pocket-notes, a tiny Node notes CLI with node:test tests and no
# dependencies), add it as project NOTE and write its tickets: standalone tickets with priorities and a
# dependency, plus two bigger tickets with a Plan of pieces. Nothing is started. Needs the service from start.sh.
# Run it once per fresh $OTAQ_DEMO; it refuses to run twice.
set -e
DEMO=${OTAQ_DEMO:-/tmp/otaqdemo}
REPO=$(git -C ${0:A:h} rev-parse --show-toplevel)
export OTAQ_HOME=$DEMO/home
O=$REPO/target/debug/otaq
R=(python3 -I ${0:A:h}/otaq_req.py)   # an array: zsh does not split "$R" into words
P=$DEMO/pocket-notes
[[ -e $P ]] && { echo "$P already exists: the demo is seeded (rm -rf $DEMO to start over)" >&2; exit 1; }
# A failure halfway leaves no half-built repo behind, so a rerun works.
trap '[[ $? -ne 0 ]] && rm -rf $P' EXIT

mkdir -p $P/src $P/test && cd $P && git init -q -b main
cat > package.json <<'EOF'
{
  "name": "pocket-notes",
  "version": "0.3.0",
  "description": "A tiny notes app for the terminal",
  "type": "module",
  "bin": { "notes": "src/cli.js" },
  "scripts": { "test": "node --test" }
}
EOF
cat > src/store.js <<'EOF'
import { readFileSync, writeFileSync, existsSync } from "node:fs";

export class Store {
  constructor(path) {
    this.path = path;
    this.notes = existsSync(path) ? JSON.parse(readFileSync(path, "utf8")) : [];
  }

  add(text) {
    const note = { id: this.notes.length + 1, text, created: new Date().toISOString() };
    this.notes.push(note);
    this.save();
    return note;
  }

  remove(id) {
    this.notes = this.notes.filter((n) => n.id !== id);
    this.save();
  }

  list() {
    return [...this.notes];
  }

  save() {
    writeFileSync(this.path, JSON.stringify(this.notes, null, 2));
  }
}
EOF
cat > src/cli.js <<'EOF'
#!/usr/bin/env node
import { homedir } from "node:os";
import { join } from "node:path";
import { Store } from "./store.js";

const store = new Store(process.env.NOTES_FILE ?? join(homedir(), ".pocket-notes.json"));
const [command, ...args] = process.argv.slice(2);

switch (command) {
  case "add":
    console.log(`Added #${store.add(args.join(" ")).id}`);
    break;
  case "rm":
    store.remove(Number(args[0]));
    break;
  case "ls":
  case undefined:
    for (const n of store.list()) console.log(`${String(n.id).padStart(3)}  ${n.text}`);
    break;
  default:
    console.error(`Unknown command: ${command}`);
    process.exit(1);
}
EOF
cat > test/store.test.js <<'EOF'
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Store } from "../src/store.js";

const fresh = () => new Store(join(mkdtempSync(join(tmpdir(), "notes-")), "notes.json"));

test("adds and lists notes", () => {
  const store = fresh();
  store.add("buy milk");
  assert.deepEqual(store.list().map((n) => n.text), ["buy milk"]);
});

test("removes a note", () => {
  const store = fresh();
  const { id } = store.add("call mom");
  store.remove(id);
  assert.equal(store.list().length, 0);
});
EOF
printf '# pocket-notes\n\nA tiny notes app for the terminal: `notes add`, `notes ls`, `notes rm`.\n' > README.md
printf 'node_modules/\n' > .gitignore
git add -A && git commit -qm "pocket-notes 0.3"

"${R[@]}" project.add "{\"path\":\"$P\",\"key\":\"NOTE\"}" >/dev/null

# Wave 1: started first (see SKILL.md, "Stage the board").
$O create -P NOTE -p 2 "Note ids are reused after a delete" --stdin <<'EOF'
`Store.add` gives a new note `notes.length + 1` as its id, so after `notes rm 1` the next note gets an id that is already taken.

**Steps**
1. `notes add a`, `notes add b`
2. `notes rm 1`
3. `notes add c` → gets id 2, the same as `b`

**Expected:** ids are never reused. Keep the next id in the file, and add a test for it.
EOF
$O create -P NOTE "Add notes search <text>" --stdin <<'EOF'
A new `search` command that lists the notes containing the text, case-insensitive, in the same format as `ls`.

- `notes search milk` → every note with "milk" in it
- no match → prints `No notes match "milk"` and exits 0
- add tests for the store method it uses
EOF
$O create -P NOTE "Show each note's age in notes ls" --deps NOTE-1 -d "Print a short age after the id: '  3  2d  buy milk'. Use m, h, d, w."
$O create -P NOTE -p 4 "Colorize output when stdout is a terminal" -d "Dim the id and age columns; no colors when piped or NO_COLOR is set."
$O create -P NOTE "Export notes to Markdown" -d "notes export > notes.md writes one bullet per note, newest first."
$O create -P NOTE -p 4 "Sync notes through a GitHub gist" -d "Investigate first: auth through the gh CLI, conflict handling."
cat > $DEMO/tags-plan.md <<'EOF'
## Pieces

1. Store tags on notes — waits for nothing
   Add a tags array to each note (empty for old notes) and Store.tagged(tag).
2. Parse #tags in notes add — waits for 1
   Words starting with # become tags and are removed from the text.
3. Filter with notes ls --tag — waits for 1
   notes ls --tag errands shows only notes with that tag.
4. Show tags in notes ls — waits for 1
   Print tags after the text, dimmed: '  3  buy milk  #errands'.
EOF
$O create -P NOTE "Tags" --plan-file $DEMO/tags-plan.md --stdin <<'EOF'
Let notes carry tags, so people can group and filter them.

`notes add buy milk #errands` stores the note with the tag `errands`; `notes ls --tag errands` shows only those.
EOF

# Wave 2: started later, so something is still working while you capture.
$O create -P NOTE -p 2 "notes edit <id> <text>" -d "Replace a note's text, keep its id, tags and created time. Unknown id: 'No note #7', exit 1."
$O create -P NOTE "Pin notes to the top of notes ls" --deps NOTE-8 -d "notes pin <id> / notes unpin <id>; pinned notes come first in ls and search."
$O create -P NOTE "Undo the last delete" -d "notes undo brings back the note removed by the last rm, with its id."
$O create -P NOTE -p 4 "Shell completions for bash and zsh" -d "notes completions bash|zsh prints a script; complete commands and note ids."
$O create -P NOTE "Fuzzy matching in notes search" -d "Rank close matches (typos, missing letters) after exact ones."
cat > $DEMO/notebooks-plan.md <<'EOF'
## Pieces

1. Store notes in notebooks — waits for nothing
   Each note gets a notebook (default 'inbox'); old files load into inbox. Store.inNotebook(name).
2. notes add -n <notebook> — waits for 1
   The -n flag picks the notebook; unknown notebooks are created.
3. notes ls -n and notes notebooks — waits for 1
   Filter ls by notebook; 'notes notebooks' lists names with counts.
4. Document notebooks in the README — waits for 2, 3
   A short section with examples.
EOF
$O create -P NOTE -p 2 "Notebooks" --plan-file $DEMO/notebooks-plan.md --stdin <<'EOF'
Group notes into notebooks (`work`, `home`), so `notes ls` stays short as the list grows.

- `notes add -n work "ship 0.4"` adds to a notebook; no `-n` means the default notebook
- `notes ls -n work` lists one notebook; `notes notebooks` lists them with counts
- old notes files load into the default notebook
EOF

$O list -P NOTE
