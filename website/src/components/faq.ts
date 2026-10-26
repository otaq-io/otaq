// Fair questions: the canvas's `FAQS` (Landing3.dc.html). Faq.astro draws each as a <details name="faq">, the first
// open.

export interface Faq {
  q: string;
  a: string;
}

export const FAQS: readonly Faq[] = [
  {
    q: "Why not tmux, a few worktrees and a script?",
    a: "That works until the third agent asks you something and you miss it. Otaq adds the parts a script doesn’t: a board that is the picture of your flow, checks that run before you see the work, a strip that tells you which one needs you, and a merge that only happens the way your flow says.",
  },
  {
    q: "Does my agent change?",
    a: "No. Otaq starts the real claude, codex, copilot or opencode program in a worktree with a ticket. Your sign-in, permission mode, skills, hooks and settings stay as they are. What Otaq writes into a tool’s config is mainly that the folders you add are trusted; Settings → Agent tools lists everything.",
  },
  {
    q: "Can it merge on its own?",
    a: "Only if you set a Delivery stage to automatic, and then only after its checks pass. The default flow always waits for your click.",
  },
  {
    q: "What does checking cost in tokens?",
    a: "A check is another agent session, so it costs what that tool charges for it. Activity (⇧⌘U) shows each tool’s tokens today and what’s left in every limit window; each ticket shows its own tokens too. You decide how many checks a flow has and which model runs each.",
  },
  {
    q: "Can my team review on GitHub instead?",
    a: "Yes. The Team PR preset has no Review stage: checks pass, a pull request opens, and your team reviews it there. Use Delivery · PR in any flow you design.",
  },
  {
    q: "Is it Mac only?",
    a: "No, but macOS is the most tested platform. Windows and Linux build and run, with less use so far. Warm builds need a filesystem that can clone copy-on-write (APFS, btrfs, XFS, ReFS); elsewhere a new worktree builds from scratch.",
  },
  {
    q: "What leaves my machine?",
    a: "Only what your agent tools, git, gh or az send, with your own sign-ins. Otaq has no account and no server. It sends nothing itself unless you opt in to anonymous usage data, and there is no update check.",
  },
];
