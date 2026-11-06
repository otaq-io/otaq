//! The bundled icon set: Lucide (ISC licence, `assets/icons/LICENSE-lucide.txt`), drawn at 14 px with a 1.5 stroke
//! (*01 · Foundations → Icons*). The SVGs come from lucide-static 1.54.0 with `stroke-width` set to 1.5; the page
//! draws them in the current text colour. The names and the SVGs live here so every UI draws the same set.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

macro_rules! icons {
    ($($variant:ident = $file:literal;)*) => {
        /// On the page, an icon is named by its Lucide name (`"bell"`); its TypeScript type lists them all.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
        pub enum IconName {
            $(#[serde(rename = $file)] $variant,)*
        }

        impl IconName {
            pub const ALL: &[IconName] = &[$(IconName::$variant),*];

            /// The Lucide name, as used in the design (`message-circle-question-mark`).
            pub fn lucide_name(self) -> &'static str {
                match self {
                    $(Self::$variant => $file,)*
                }
            }

            /// The asset path, as passed to `svg().path(...)`.
            pub fn path(self) -> &'static str {
                match self {
                    $(Self::$variant => concat!("icons/", $file, ".svg"),)*
                }
            }

            /// The SVG bytes, embedded at build time.
            pub fn bytes(self) -> &'static [u8] {
                match self {
                    $(Self::$variant => include_bytes!(concat!("../assets/icons/", $file, ".svg")),)*
                }
            }
        }
    };
}

icons! {
    AArrowDown = "a-arrow-down";
    AArrowUp = "a-arrow-up";
    ALargeSmall = "a-large-small";
    Activity = "activity";
    AlarmClock = "alarm-clock";
    Archive = "archive";
    ArrowDown = "arrow-down";
    ArrowDownToLine = "arrow-down-to-line";
    ArrowLeft = "arrow-left";
    ArrowRight = "arrow-right";
    ArrowRightLeft = "arrow-right-left";
    ArrowUp = "arrow-up";
    ArrowUpRight = "arrow-up-right";
    Ban = "ban";
    Bell = "bell";
    BetweenHorizontalEnd = "between-horizontal-end";
    BetweenHorizontalStart = "between-horizontal-start";
    BetweenVerticalEnd = "between-vertical-end";
    BetweenVerticalStart = "between-vertical-start";
    Bold = "bold";
    BookOpen = "book-open";
    Bot = "bot";
    BrushCleaning = "brush-cleaning";
    CalendarClock = "calendar-clock";
    Check = "check";
    CheckCheck = "check-check";
    ChevronDown = "chevron-down";
    ChevronLeft = "chevron-left";
    ChevronRight = "chevron-right";
    ChevronUp = "chevron-up";
    ChevronsDownUp = "chevrons-down-up";
    ChevronsLeftRight = "chevrons-left-right";
    ChevronsUp = "chevrons-up";
    ChevronsRight = "chevrons-right";
    ChevronsUpDown = "chevrons-up-down";
    Circle = "circle";
    CircleAlert = "circle-alert";
    CircleCheck = "circle-check";
    CircleCheckBig = "circle-check-big";
    CircleDashed = "circle-dashed";
    CircleMinus = "circle-minus";
    CirclePause = "circle-pause";
    CirclePlay = "circle-play";
    CircleQuestionMark = "circle-question-mark";
    CircleSlash = "circle-slash";
    CircleStop = "circle-stop";
    CircleX = "circle-x";
    ClipboardCopy = "clipboard-copy";
    Clock = "clock";
    Code = "code";
    Coins = "coins";
    Columns2 = "columns-2";
    Columns3 = "columns-3";
    Copy = "copy";
    CornerDownLeft = "corner-down-left";
    CornerDownRight = "corner-down-right";
    CornerUpLeft = "corner-up-left";
    Cpu = "cpu";
    Database = "database";
    Dot = "dot";
    Download = "download";
    Ellipsis = "ellipsis";
    Equal = "equal";
    ExternalLink = "external-link";
    Eye = "eye";
    EyeOff = "eye-off";
    File = "file";
    FileDiff = "file-diff";
    FileSymlink = "file-symlink";
    FileText = "file-text";
    Files = "files";
    Flame = "flame";
    FoldHorizontal = "fold-horizontal";
    Folder = "folder";
    FolderTree = "folder-tree";
    FolderGit2 = "folder-git-2";
    FolderInput = "folder-input";
    FolderOpen = "folder-open";
    FolderPlus = "folder-plus";
    FolderX = "folder-x";
    Gauge = "gauge";
    GitBranch = "git-branch";
    GitCommitHorizontal = "git-commit-horizontal";
    GitCompare = "git-compare";
    GitCompareArrows = "git-compare-arrows";
    GitMerge = "git-merge";
    GitPullRequest = "git-pull-request";
    Globe = "globe";
    GripVertical = "grip-vertical";
    Hand = "hand";
    Hash = "hash";
    History = "history";
    Hourglass = "hourglass";
    Image = "image";
    Inbox = "inbox";
    Info = "info";
    Italic = "italic";
    Keyboard = "keyboard";
    Layers = "layers";
    LayoutDashboard = "layout-dashboard";
    Lightbulb = "lightbulb";
    Link = "link";
    Link2 = "link-2";
    List = "list";
    ListChecks = "list-checks";
    ListOrdered = "list-ordered";
    ListTodo = "list-todo";
    Loader = "loader";
    LoaderCircle = "loader-circle";
    Lock = "lock";
    LogIn = "log-in";
    Mail = "mail";
    Maximize = "maximize";
    Maximize2 = "maximize-2";
    MessageCircle = "message-circle";
    MessageCircleQuestionMark = "message-circle-question-mark";
    MessageCircleReply = "message-circle-reply";
    MessageSquare = "message-square";
    MessageSquareDot = "message-square-dot";
    MessageSquareText = "message-square-text";
    MessagesSquare = "messages-square";
    Minimize = "minimize";
    Minimize2 = "minimize-2";
    Minus = "minus";
    MousePointerClick = "mouse-pointer-click";
    Palette = "palette";
    PanelLeftClose = "panel-left-close";
    PanelRight = "panel-right";
    Paperclip = "paperclip";
    Pause = "pause";
    Pencil = "pencil";
    Pin = "pin";
    PinOff = "pin-off";
    Play = "play";
    Plug = "plug";
    Plus = "plus";
    RefreshCcw = "refresh-ccw";
    RefreshCw = "refresh-cw";
    RotateCcw = "rotate-ccw";
    RotateCw = "rotate-cw";
    ScanEye = "scan-eye";
    ScanSearch = "scan-search";
    ScrollText = "scroll-text";
    Search = "search";
    SearchX = "search-x";
    Send = "send";
    SendHorizontal = "send-horizontal";
    Server = "server";
    ServerCog = "server-cog";
    ServerOff = "server-off";
    Settings = "settings";
    Shield = "shield";
    ShieldAlert = "shield-alert";
    ShieldCheck = "shield-check";
    ShieldEllipsis = "shield-ellipsis";
    ShieldOff = "shield-off";
    ShieldQuestionMark = "shield-question-mark";
    ShieldX = "shield-x";
    Signpost = "signpost";
    SkipForward = "skip-forward";
    Sparkles = "sparkles";
    Square = "square";
    SquareCheck = "square-check";
    SquareCode = "square-code";
    SquareDashed = "square-dashed";
    SquareParking = "square-parking";
    SquarePlus = "square-plus";
    SquareTerminal = "square-terminal";
    Star = "star";
    Strikethrough = "strikethrough";
    Table = "table";
    Tag = "tag";
    Terminal = "terminal";
    TextQuote = "text-quote";
    Ticket = "ticket";
    Trash2 = "trash-2";
    TriangleAlert = "triangle-alert";
    Undo2 = "undo-2";
    Unlink = "unlink";
    Upload = "upload";
    User = "user";
    UserCheck = "user-check";
    Workflow = "workflow";
    Wrench = "wrench";
    X = "x";
    ZapOff = "zap-off";
}

impl IconName {
    pub fn from_path(path: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|i| i.path() == path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_bundled_with_a_thin_stroke() {
        assert!(IconName::ALL.len() >= 70);
        for icon in IconName::ALL {
            let text = std::str::from_utf8(icon.bytes()).unwrap();
            assert!(text.contains("<svg"), "{icon:?}");
            assert!(text.contains(r#"stroke-width="1.5""#), "{icon:?}");
            let shapes = [
                "<path",
                "<circle",
                "<rect",
                "<line",
                "<polyline",
                "<polygon",
                "<ellipse",
            ];
            assert!(
                shapes.iter().any(|s| text.contains(s)),
                "{icon:?} draws nothing"
            );
            assert_eq!(IconName::from_path(icon.path()), Some(*icon));
        }
    }

    /// Every Lucide icon `crewboard.pen` uses (read from the design on 2026-10-11, after design update M, plus the
    /// Markdown toolbar's from update P on 2026-10-15), except `github`: Lucide dropped it and the app shows
    /// `git-pull-request` instead (Q31). An icon added to the design shows up here as missing until it is bundled.
    const DESIGN_ICONS: &[&str] = &[
        "a-arrow-down",
        "a-arrow-up",
        "a-large-small",
        "activity",
        "alarm-clock",
        "archive",
        "arrow-down",
        "arrow-down-to-line",
        "arrow-left",
        "arrow-right",
        "arrow-up",
        "arrow-up-right",
        "ban",
        "bell",
        "between-horizontal-end",
        "between-horizontal-start",
        "between-vertical-end",
        "between-vertical-start",
        "bold",
        "bot",
        "brush-cleaning",
        "calendar-clock",
        "check",
        "check-check",
        "chevron-down",
        "chevron-right",
        "chevron-up",
        "chevrons-down-up",
        "chevrons-left-right",
        "chevrons-up",
        "chevrons-up-down",
        "circle",
        "circle-alert",
        "circle-check",
        "circle-check-big",
        "circle-dashed",
        "circle-minus",
        "circle-pause",
        "circle-play",
        "circle-question-mark",
        "circle-slash",
        "circle-stop",
        "circle-x",
        "clipboard-copy",
        "code",
        "coins",
        "copy",
        "corner-down-left",
        "corner-down-right",
        "corner-up-left",
        "cpu",
        "database",
        "dot",
        "ellipsis",
        "equal",
        "external-link",
        "eye",
        "eye-off",
        "file",
        "file-diff",
        "file-symlink",
        "file-text",
        "files",
        "flame",
        "folder",
        "folder-git-2",
        "folder-open",
        "folder-plus",
        "gauge",
        "git-branch",
        "git-commit-horizontal",
        "git-compare",
        "git-compare-arrows",
        "git-merge",
        "git-pull-request",
        "globe",
        "grip-vertical",
        "hand",
        "hash",
        "history",
        "hourglass",
        "image",
        "inbox",
        "info",
        "italic",
        "keyboard",
        "layers",
        "layout-dashboard",
        "link",
        "link-2",
        "list",
        "list-checks",
        "list-ordered",
        "list-todo",
        "loader",
        "loader-circle",
        "lock",
        "mail",
        "maximize",
        "maximize-2",
        "message-circle-question-mark",
        "message-circle-reply",
        "message-square",
        "message-square-dot",
        "message-square-text",
        "messages-square",
        "minimize",
        "minimize-2",
        "minus",
        "mouse-pointer-click",
        "palette",
        "panel-right",
        "pencil",
        "pin",
        "pin-off",
        "play",
        "plug",
        "plus",
        "refresh-ccw",
        "refresh-cw",
        "rotate-ccw",
        "rotate-cw",
        "scan-search",
        "scroll-text",
        "search",
        "send",
        "server",
        "server-cog",
        "server-off",
        "settings",
        "shield",
        "shield-alert",
        "shield-check",
        "shield-ellipsis",
        "shield-off",
        "shield-question-mark",
        "shield-x",
        "signpost",
        "skip-forward",
        "sparkles",
        "square",
        "square-check",
        "square-code",
        "square-parking",
        "square-plus",
        "square-terminal",
        "star",
        "strikethrough",
        "table",
        "tag",
        "terminal",
        "text-quote",
        "trash-2",
        "triangle-alert",
        "undo-2",
        "unlink",
        "upload",
        "user",
        "user-check",
        "workflow",
        "wrench",
        "x",
        "zap-off",
    ];

    #[test]
    fn every_design_icon_is_bundled() {
        let missing: Vec<_> = DESIGN_ICONS
            .iter()
            .filter(|name| !IconName::ALL.iter().any(|i| i.lucide_name() == **name))
            .collect();
        assert!(missing.is_empty(), "design icons not bundled: {missing:?}");
    }

    #[test]
    fn names_match_lucide() {
        assert_eq!(
            IconName::MessageCircleQuestionMark.lucide_name(),
            "message-circle-question-mark"
        );
        assert_eq!(IconName::Hand.path(), "icons/hand.svg");
    }
}
