//! The visual language from *crewboard.pen → 01 · Foundations* and *01 · Theme palettes* (SDD §21.2, brief 01,
//! d0069) as plain data: the 15 bundled themes (49 colour tokens and 16 terminal colours each, as RGBA hex), the
//! status legend (tier, palette, icon and label; colour is never the only signal), the fonts, spacing, radius and
//! density. Each UI turns the hex values into its own colour type; every screen reads its colours from the active
//! theme, never from literals.
//!
//! The user picks one theme and it is used all the time (d0070, stored in the global settings). Until one is
//! picked, the theme follows the OS at start-up ([`ThemeId::from_setting`]).

use otaq_core::board::settings::{Theme as StoredTheme, ThemeSetting};
use otaq_core::naming;
use serde::{Serialize, Serializer};
use ts_rs::TS;

use crate::icons::IconName;

/// Whether a window is drawn light or dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Light,
    Dark,
}

/// The 15 bundled themes (d0069, d0070), in the order of the design's `palette` axis, Otaq's two versions first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeId {
    OtaqLight,
    OtaqDark,
    CatppuccinFrappe,
    CatppuccinMacchiato,
    CatppuccinMocha,
    TokyoNight,
    Dracula,
    Nord,
    GruvboxDark,
    RosePine,
    OneDark,
    CatppuccinLatte,
    Daylight,
    SolarizedLight,
    RosePineDawn,
}

/// A theme goes to the page as its key (`tokyo-night`), the value of `data-theme`.
impl Serialize for ThemeId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.stored().as_str())
    }
}

impl ThemeId {
    /// Every theme, in the order of the token table (the pickers use [`Self::DARK`] and [`Self::LIGHT`]).
    pub const ALL: [ThemeId; 15] = [
        Self::OtaqLight,
        Self::OtaqDark,
        Self::CatppuccinFrappe,
        Self::CatppuccinMacchiato,
        Self::CatppuccinMocha,
        Self::TokyoNight,
        Self::Dracula,
        Self::Nord,
        Self::GruvboxDark,
        Self::RosePine,
        Self::OneDark,
        Self::CatppuccinLatte,
        Self::Daylight,
        Self::SolarizedLight,
        Self::RosePineDawn,
    ];

    /// The dark themes, as the pickers list them (*01 · Theme palettes → Dark themes*).
    pub const DARK: [ThemeId; 10] = [
        Self::OtaqDark,
        Self::CatppuccinFrappe,
        Self::CatppuccinMacchiato,
        Self::CatppuccinMocha,
        Self::TokyoNight,
        Self::Dracula,
        Self::Nord,
        Self::GruvboxDark,
        Self::RosePine,
        Self::OneDark,
    ];

    /// The light themes, as the pickers list them (*01 · Theme palettes → Light themes*).
    pub const LIGHT: [ThemeId; 5] = [
        Self::OtaqLight,
        Self::CatppuccinLatte,
        Self::Daylight,
        Self::SolarizedLight,
        Self::RosePineDawn,
    ];

    /// The stored theme as a theme.
    pub fn from_stored(theme: StoredTheme) -> Self {
        match theme {
            StoredTheme::OtaqDark => Self::OtaqDark,
            StoredTheme::CatppuccinFrappe => Self::CatppuccinFrappe,
            StoredTheme::CatppuccinMacchiato => Self::CatppuccinMacchiato,
            StoredTheme::CatppuccinMocha => Self::CatppuccinMocha,
            StoredTheme::TokyoNight => Self::TokyoNight,
            StoredTheme::Dracula => Self::Dracula,
            StoredTheme::Nord => Self::Nord,
            StoredTheme::GruvboxDark => Self::GruvboxDark,
            StoredTheme::RosePine => Self::RosePine,
            StoredTheme::OneDark => Self::OneDark,
            StoredTheme::OtaqLight => Self::OtaqLight,
            StoredTheme::CatppuccinLatte => Self::CatppuccinLatte,
            StoredTheme::Daylight => Self::Daylight,
            StoredTheme::SolarizedLight => Self::SolarizedLight,
            StoredTheme::RosePineDawn => Self::RosePineDawn,
        }
    }

    /// The theme's setting value.
    pub fn stored(self) -> StoredTheme {
        StoredTheme::ALL
            .iter()
            .copied()
            .find(|t| Self::from_stored(*t) == self)
            .expect("every theme is stored")
    }

    /// The theme to draw with: the picked one, or, until one is picked, the one for the OS (window) appearance.
    pub fn from_setting(setting: ThemeSetting, os: Appearance) -> Self {
        Self::from_stored(setting.resolve(os == Appearance::Dark))
    }

    pub fn appearance(self) -> Appearance {
        if self.stored().is_dark() {
            Appearance::Dark
        } else {
            Appearance::Light
        }
    }

    /// The name shown in the pickers and the credits.
    pub fn name(self) -> String {
        match self {
            Self::OtaqLight => format!("{} Light", naming::PRODUCT_NAME),
            Self::OtaqDark => format!("{} Dark", naming::PRODUCT_NAME),
            Self::CatppuccinFrappe => "Catppuccin Frappé".into(),
            Self::CatppuccinMacchiato => "Catppuccin Macchiato".into(),
            Self::CatppuccinMocha => "Catppuccin Mocha".into(),
            Self::TokyoNight => "Tokyo Night".into(),
            Self::Dracula => "Dracula".into(),
            Self::Nord => "Nord".into(),
            Self::GruvboxDark => "Gruvbox Dark".into(),
            Self::RosePine => "Rosé Pine".into(),
            Self::OneDark => "One Dark".into(),
            Self::CatppuccinLatte => "Catppuccin Latte".into(),
            Self::Daylight => "Daylight".into(),
            Self::SolarizedLight => "Solarized Light".into(),
            Self::RosePineDawn => "Rosé Pine Dawn".into(),
        }
    }
}

macro_rules! color_tokens {
    ($($field:ident = $name:literal : [$($value:literal),+];)*) => {
        /// Every colour token of *01 · Foundations → Color Tokens*, resolved for one theme: as RGBA hex (`C = u32`,
        /// [`Colors::new`]), or in a UI's own colour type ([`Colors::map`]).
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct Colors<C = u32> {
            $(pub $field: C,)*
        }

        impl Colors {
            /// The theme's tokens as RGBA hex (`0xRRGGBBAA`).
            pub fn new(theme: ThemeId) -> Self {
                let i = theme as usize;
                Self { $($field: [$($value),+][i],)* }
            }
        }

        impl<C: Copy> Colors<C> {
            /// Token names as they appear in the design (`bg-app`, `st-attn`, …).
            pub const NAMES: &[&str] = &[$($name),*];

            /// Looks a token up by its design name.
            pub fn get(&self, name: &str) -> Option<C> {
                match name {
                    $($name => Some(self.$field),)*
                    _ => None,
                }
            }

            /// Every token converted, for example to the UI's colour type.
            pub fn map<D>(&self, f: impl Fn(C) -> D) -> Colors<D> {
                Colors { $($field: f(self.$field),)* }
            }
        }
    };
}

// Values as RGBA hex, copied from the design's variables (*01 · Theme palettes*), one column per theme in the order
// of `ThemeId::ALL`: Otaq Light, Otaq Dark, Catppuccin Frappé, Macchiato, Mocha, Tokyo Night, Dracula,
// Nord, Gruvbox Dark, Rosé Pine, One Dark, Catppuccin Latte, Daylight, Solarized Light, Rosé Pine Dawn.
color_tokens! {
    bg_app = "bg-app": [0xF6F6F4FF, 0x0E0F11FF, 0x232634FF, 0x181926FF, 0x11111BFF, 0x16161EFF, 0x191A21FF, 0x242933FF, 0x1D2021FF, 0x13111DFF, 0x1B1D23FF, 0xE6E9EFFF, 0xF6F8FAFF, 0xF7F0DDFF, 0xFAF4EDFF];
    bg_panel = "bg-panel": [0xFFFFFFFF, 0x15161AFF, 0x303446FF, 0x24273AFF, 0x1E1E2EFF, 0x1A1B26FF, 0x282A36FF, 0x2E3440FF, 0x282828FF, 0x1F1D2EFF, 0x282C34FF, 0xEFF1F5FF, 0xFFFFFFFF, 0xFDF6E3FF, 0xFFFAF3FF];
    bg_column = "bg-column": [0xEEEEEBFF, 0x121316FF, 0x292C3CFF, 0x1E2030FF, 0x181825FF, 0x191A24FF, 0x21222CFF, 0x292E39FF, 0x252423FF, 0x191724FF, 0x21252BFF, 0xDCE0E8FF, 0xEFF2F5FF, 0xEEE8D5FF, 0xF2E9E1FF];
    bg_card = "bg-card": [0xFFFFFFFF, 0x1C1E23FF, 0x303446FF, 0x24273AFF, 0x1E1E2EFF, 0x1F2335FF, 0x2F3141FF, 0x333946FF, 0x32302FFF, 0x26233AFF, 0x2C313CFF, 0xEFF1F5FF, 0xFFFFFFFF, 0xFDF6E3FF, 0xFFFAF3FF];
    bg_raised = "bg-raised": [0xFFFFFFFF, 0x23262CFF, 0x414559FF, 0x363A4FFF, 0x313244FF, 0x24283BFF, 0x343746FF, 0x3B4252FF, 0x3C3836FF, 0x26233AFF, 0x323842FF, 0xEFF1F5FF, 0xFFFFFFFF, 0xFDF6E3FF, 0xFFFAF3FF];
    bg_hover = "bg-hover": [0xEDEDEAFF, 0x262930FF, 0x414559FF, 0x363A4FFF, 0x313244FF, 0x292E42FF, 0x3A3D4EFF, 0x434C5EFF, 0x3C3836FF, 0x2A2740FF, 0x353B45FF, 0xE6E9EFFF, 0xF3F5F7FF, 0xF5EFDCFF, 0xF4EDE8FF];
    bg_pressed = "bg-pressed": [0xE4E4E0FF, 0x2E323AFF, 0x51576DFF, 0x494D64FF, 0x45475AFF, 0x343A55FF, 0x44475AFF, 0x4C566AFF, 0x504945FF, 0x403D52FF, 0x3E4451FF, 0xDCE0E8FF, 0xEAEEF2FF, 0xEEE8D5FF, 0xEFE6DEFF];
    border = "border": [0xE2E2DEFF, 0x272A31FF, 0x414559FF, 0x363A4FFF, 0x313244FF, 0x292E42FF, 0x343746FF, 0x3B4252FF, 0x3C3836FF, 0x2A2740FF, 0x3A3F4BFF, 0xCCD0DAFF, 0xD0D7DEFF, 0xE4DDC8FF, 0xDFDAD9FF];
    border_strong = "border-strong": [0x959588FF, 0x686F7FFF, 0x51576DFF, 0x494D64FF, 0x45475AFF, 0x3B4261FF, 0x565A70FF, 0x4C566AFF, 0x504945FF, 0x403D52FF, 0x4B5263FF, 0xBCC0CCFF, 0xAFB8C1FF, 0xD3CBB3FF, 0xCECACDFF];
    border_card = "border-card": [0x8A8A7AFF, 0x62697BFF, 0x414559FF, 0x363A4FFF, 0x313244FF, 0x292E42FF, 0x343746FF, 0x3B4252FF, 0x3C3836FF, 0x2A2740FF, 0x3A3F4BFF, 0xCCD0DAFF, 0xD0D7DEFF, 0xE4DDC8FF, 0xDFDAD9FF];
    text_primary = "text-primary": [0x1A1B1EFF, 0xECEDEFFF, 0xC6D0F5FF, 0xCAD3F5FF, 0xCDD6F4FF, 0xC0CAF5FF, 0xF8F8F2FF, 0xECEFF4FF, 0xEBDBB2FF, 0xE0DEF4FF, 0xD7DAE0FF, 0x4C4F69FF, 0x1F2328FF, 0x586E75FF, 0x575279FF];
    text_secondary = "text-secondary": [0x5B5F67FF, 0xA5A9B2FF, 0xA5ADCEFF, 0xA5ADCBFF, 0xA6ADC8FF, 0xA9B1D6FF, 0xC3C5D4FF, 0xD8DEE9FF, 0xBDAE93FF, 0x908CAAFF, 0xABB2BFFF, 0x6C6F85FF, 0x59636EFF, 0x657B83FF, 0x797593FF];
    text_muted = "text-muted": [0x676B74FF, 0x8C9099FF, 0x838BA7FF, 0x8087A2FF, 0x7F849CFF, 0x737AA2FF, 0x7B86B5FF, 0x7B88A1FF, 0x928374FF, 0x6E6A86FF, 0x7F848EFF, 0x8C8FA1FF, 0x818B98FF, 0x93A1A1FF, 0x9893A5FF];
    text_on_accent = "text-on-accent": [0xFFFFFFFF, 0x0E0F11FF, 0x232634FF, 0x181926FF, 0x11111BFF, 0x1A1B26FF, 0x282A36FF, 0x2E3440FF, 0x282828FF, 0x191724FF, 0x282C34FF, 0xEFF1F5FF, 0xFFFFFFFF, 0xFDF6E3FF, 0xFFFAF3FF];
    accent = "accent": [0x4C57D6FF, 0x757FF7FF, 0xCA9EE6FF, 0xC6A0F6FF, 0xCBA6F7FF, 0x7AA2F7FF, 0xBD93F9FF, 0x88C0D0FF, 0x83A598FF, 0xC4A7E7FF, 0x61AFEFFF, 0x8839EFFF, 0x0969DAFF, 0x268BD2FF, 0x907AA9FF];
    accent_hover = "accent-hover": [0x3F49C2FF, 0x828BFFFF, 0xD2ADEAFF, 0xCFAEF7FF, 0xD3B3F8FF, 0x8EB0F8FF, 0xC7A3FAFF, 0x9AC9D7FF, 0x96B3A7FF, 0xCDB4EBFF, 0x79BBF1FF, 0x7430CBFF, 0x0859B9FF, 0x2076B3FF, 0x7A6890FF];
    accent_soft = "accent-soft": [0xECEEFDFF, 0x1E2142FF, 0x4C4763FF, 0x413D5CFF, 0x3D3652FF, 0x2B334CFF, 0x433D59FF, 0x3E4D5AFF, 0x383F3CFF, 0x3D364FFF, 0x324456FF, 0xE5DFF4FF, 0xE6F0FBFF, 0xE8EBE1FF, 0xF4EDECFF];
    focus = "focus": [0x4C57D6FF, 0x8A93FFFF, 0xCA9EE6FF, 0xC6A0F6FF, 0xCBA6F7FF, 0x7AA2F7FF, 0xBD93F9FF, 0x88C0D0FF, 0x83A598FF, 0xC4A7E7FF, 0x61AFEFFF, 0x8839EFFF, 0x0969DAFF, 0x268BD2FF, 0x907AA9FF];
    selection = "selection": [0x4C57D633, 0x6E78F74D, 0xCA9EE64D, 0xC6A0F64D, 0xCBA6F74D, 0x7AA2F74D, 0xBD93F94D, 0x88C0D04D, 0x83A5984D, 0xC4A7E74D, 0x61AFEF4D, 0x8839EF33, 0x0969DA33, 0x268BD233, 0x907AA933];
    st_attn = "st-attn": [0x087A6FFF, 0x2DD4BFFF, 0x81C8BEFF, 0x8BD5CAFF, 0x94E2D5FF, 0x73DACAFF, 0x40E0D0FF, 0x8FBCBBFF, 0x8EC07CFF, 0x9CCFD8FF, 0x56B6C2FF, 0x179299FF, 0x1B7C83FF, 0x2AA198FF, 0x56949FFF];
    st_attn_bg = "st-attn-bg": [0xE0F5F2FF, 0x0E2E2AFF, 0x3F4F5CFF, 0x374654FF, 0x33414CFF, 0x2A3D44FF, 0x2C4B52FF, 0x3F4C56FF, 0x3A4337FF, 0x363D4DFF, 0x30454EFF, 0xD9E8ECFF, 0xE8F2F3FF, 0xE8EEDCFF, 0xEEF0EBFF];
    on_attn = "on-attn": [0xFFFFFFFF, 0x0E0F11FF, 0x232634FF, 0x181926FF, 0x11111BFF, 0x1A1B26FF, 0x282A36FF, 0x2E3440FF, 0x282828FF, 0x191724FF, 0x282C34FF, 0xEFF1F5FF, 0xFFFFFFFF, 0xFDF6E3FF, 0xFFFAF3FF];
    st_error = "st-error": [0xCC2F3BFF, 0xFF6670FF, 0xE78284FF, 0xED8796FF, 0xF38BA8FF, 0xF7768EFF, 0xFF5555FF, 0xBF616AFF, 0xFB4934FF, 0xEB6F92FF, 0xE06C75FF, 0xD20F39FF, 0xD1242FFF, 0xDC322FFF, 0xB4637AFF];
    st_error_bg = "st-error-bg": [0xFDEBEDFF, 0x3A191CFF, 0x514251FF, 0x48384BFF, 0x443244FF, 0x422B39FF, 0x4F323CFF, 0x483C48FF, 0x4E2E2AFF, 0x442C40FF, 0x493840FF, 0xECD8E0FF, 0xFAE7E8FF, 0xF9E0CFFF, 0xF7E9E6FF];
    st_working = "st-working": [0x2A68D6FF, 0x6AA6FFFF, 0x8CAAEEFF, 0x8AADF4FF, 0x89B4FAFF, 0x7DCFFFFF, 0x8BE9FDFF, 0x81A1C1FF, 0x83A598FF, 0x3E8FB0FF, 0x61AFEFFF, 0x1E66F5FF, 0x0969DAFF, 0x268BD2FF, 0x286983FF];
    st_working_bg = "st-working-bg": [0xEAF1FDFF, 0x15233BFF, 0x414964FF, 0x363F5BFF, 0x313953FF, 0x2C3B4DFF, 0x3A4C5AFF, 0x3D4857FF, 0x383F3CFF, 0x253245FF, 0x324456FF, 0xD8E2F5FF, 0xE4EFFBFF, 0xE5EAE1FF, 0xE7EAE7FF];
    st_investigate = "st-investigate": [0x7650D2FF, 0xB196FFFF, 0xBABBF1FF, 0xB7BDF8FF, 0xB4BEFEFF, 0xBB9AF7FF, 0xD6ACFFFF, 0xB48EADFF, 0xA89ADBFF, 0xC4A7E7FF, 0xC678DDFF, 0x8839EFFF, 0x8250DFFF, 0x6C71C4FF, 0x907AA9FF];
    st_investigate_bg = "st-investigate-bg": [0xF2EEFCFF, 0x251C3BFF, 0x494C65FF, 0x3E425CFF, 0x393B53FF, 0x37324CFF, 0x47415AFF, 0x464454FF, 0x3F3D48FF, 0x3D364FFF, 0x443A52FF, 0xE5DFF4FF, 0xF1ECFBFF, 0xEDE7E0FF, 0xF3ECEBFF];
    st_queued = "st-queued": [0x646A75FF, 0x9AA0ABFF, 0x949CBBFF, 0x939AB7FF, 0x9399B2FF, 0x828BB8FF, 0x9AA3C4FF, 0x8E9AAFFF, 0xA89984FF, 0x908CAAFF, 0x9DA5B4FF, 0x7C7F93FF, 0x59636EFF, 0x839496FF, 0x797593FF];
    st_queued_bg = "st-queued-bg": [0xEFF0F2FF, 0x23262CFF, 0x3E4356FF, 0x34374CFF, 0x2E2F40FF, 0x292B3AFF, 0x383B4AFF, 0x3B4250FF, 0x3A3835FF, 0x2F2D3FFF, 0x383D46FF, 0xE4E6EBFF, 0xEEEFF1FF, 0xF1ECDBFF, 0xF2EDE9FF];
    st_review = "st-review": [0xBA3888FF, 0xF07CC4FF, 0xF4B8E4FF, 0xF5BDE6FF, 0xF5C2E7FF, 0xF77FD6FF, 0xFF79C6FF, 0xDC94BDFF, 0xD3869BFF, 0xEC9BD1FF, 0xE77FC0FF, 0xEA76CBFF, 0xBF3989FF, 0xD33682FF, 0xB85FA8FF];
    st_review_bg = "st-review-bg": [0xFBEAF4FF, 0x3A1830FF, 0x534C62FF, 0x4A4259FF, 0x453C4FFF, 0x422D46FF, 0x4F3850FF, 0x4D4557FF, 0x47393DFF, 0x44344BFF, 0x4A3B4DFF, 0xEFE5F1FF, 0xF9EBF3FF, 0xF9E3D9FF, 0xF8EBECFF];
    st_paused = "st-paused": [0x976400FF, 0xF2BE4AFF, 0xE5C890FF, 0xEED49FFF, 0xF9E2AFFF, 0xE0AF68FF, 0xF1FA8CFF, 0xEBCB8BFF, 0xFABD2FFF, 0xEBBCBAFF, 0xE5C07BFF, 0xDF8E1DFF, 0x9A6700FF, 0xB58900FF, 0xD7827EFF];
    st_paused_bg = "st-paused-bg": [0xFAF2DAFF, 0x33290EFF, 0x514F53FF, 0x48464CFF, 0x454145FF, 0x3E3632FF, 0x4C4F45FF, 0x504F4EFF, 0x4E4329FF, 0x443A47FF, 0x4A4741FF, 0xEDE6DDFF, 0xF4EEE3FF, 0xF5EACAFF, 0xFBEDE6FF];
    st_approved = "st-approved": [0x237D46FF, 0x4CC77AFF, 0xA6D189FF, 0xA6DA95FF, 0xA6E3A1FF, 0x9ECE6AFF, 0x50FA7BFF, 0xA3BE8CFF, 0xB8BB26FF, 0xA3C9A8FF, 0x98C379FF, 0x40A02BFF, 0x1A7F37FF, 0x859900FF, 0x5F8F6AFF];
    st_approved_bg = "st-approved-bg": [0xE6F4EAFF, 0x12301CFF, 0x455052FF, 0x3B474AFF, 0x364143FF, 0x323B32FF, 0x2F4F42FF, 0x434D4EFF, 0x424228FF, 0x373C44FF, 0x3C4740FF, 0xDCE8DFFF, 0xE6F1E9FF, 0xF0ECCAFF, 0xEDEEE4FF];
    st_done = "st-done": [0x868992FF, 0x6A6F79FF, 0x737994FF, 0x6E738DFF, 0x6C7086FF, 0x565F89FF, 0x6272A4FF, 0x616E88FF, 0x7C6F64FF, 0x524F67FF, 0x5C6370FF, 0x9CA0B0FF, 0x8C959FFF, 0x93A1A1FF, 0x9893A5FF];
    term_bg = "term-bg": [0xFAFAF8FF, 0x0A0B0DFF, 0x303446FF, 0x24273AFF, 0x1E1E2EFF, 0x1A1B26FF, 0x282A36FF, 0x2E3440FF, 0x282828FF, 0x191724FF, 0x282C34FF, 0xEFF1F5FF, 0xF6F8FAFF, 0xFDF6E3FF, 0xFAF4EDFF];
    term_fg = "term-fg": [0x2A2C31FF, 0xD6D8DDFF, 0xC6D0F5FF, 0xCAD3F5FF, 0xCDD6F4FF, 0xC0CAF5FF, 0xF8F8F2FF, 0xD8DEE9FF, 0xEBDBB2FF, 0xE0DEF4FF, 0xABB2BFFF, 0x4C4F69FF, 0x1F2328FF, 0x657B83FF, 0x575279FF];
    diff_add_bg = "diff-add-bg": [0xE5F6EAFF, 0x11291AFF, 0x434D51FF, 0x394449FF, 0x343E40FF, 0x2F3831FF, 0x2E4B41FF, 0x414A4CFF, 0x3F4028FF, 0x2F3339FF, 0x3A443FFF, 0xDAE7DDFF, 0xDCE9E3FF, 0xEFEBC8FF, 0xE7E8DDFF];
    diff_del_bg = "diff-del-bg": [0xFCEBECFF, 0x2E1517FF, 0x4D4050FF, 0x443649FF, 0x402F42FF, 0x3D2A37FF, 0x4A313BFF, 0x453B47FF, 0x4A2D2AFF, 0x3B2536FF, 0x45363EFF, 0xECD6DEFF, 0xF2DFE2FF, 0xF9DECDFF, 0xF2E3DFFF];
    // The tool avatars (SHL-08, d0087; *01 · Foundations → Update U + V · Tool avatars*): one background and one
    // letter colour per agent tool, the light value in the five light themes and the dark one in the other ten.
    tool_cc = "tool-cc": [0xE4E9CFFF, 0x3A4220FF, 0x3A4220FF, 0x3A4220FF, 0x3A4220FF, 0x3A4220FF, 0x3A4220FF, 0x3A4220FF, 0x3A4220FF, 0x3A4220FF, 0x3A4220FF, 0xE4E9CFFF, 0xE4E9CFFF, 0xE4E9CFFF, 0xE4E9CFFF];
    tool_cc_fg = "tool-cc-fg": [0x4A5A12FF, 0xD3DFA0FF, 0xD3DFA0FF, 0xD3DFA0FF, 0xD3DFA0FF, 0xD3DFA0FF, 0xD3DFA0FF, 0xD3DFA0FF, 0xD3DFA0FF, 0xD3DFA0FF, 0xD3DFA0FF, 0x4A5A12FF, 0x4A5A12FF, 0x4A5A12FF, 0x4A5A12FF];
    tool_cx = "tool-cx": [0xDCE3ECFF, 0x2B3647FF, 0x2B3647FF, 0x2B3647FF, 0x2B3647FF, 0x2B3647FF, 0x2B3647FF, 0x2B3647FF, 0x2B3647FF, 0x2B3647FF, 0x2B3647FF, 0xDCE3ECFF, 0xDCE3ECFF, 0xDCE3ECFF, 0xDCE3ECFF];
    tool_cx_fg = "tool-cx-fg": [0x2E405AFF, 0xBFD0E8FF, 0xBFD0E8FF, 0xBFD0E8FF, 0xBFD0E8FF, 0xBFD0E8FF, 0xBFD0E8FF, 0xBFD0E8FF, 0xBFD0E8FF, 0xBFD0E8FF, 0xBFD0E8FF, 0x2E405AFF, 0x2E405AFF, 0x2E405AFF, 0x2E405AFF];
    tool_cp = "tool-cp": [0xEDE3D3FF, 0x43352AFF, 0x43352AFF, 0x43352AFF, 0x43352AFF, 0x43352AFF, 0x43352AFF, 0x43352AFF, 0x43352AFF, 0x43352AFF, 0x43352AFF, 0xEDE3D3FF, 0xEDE3D3FF, 0xEDE3D3FF, 0xEDE3D3FF];
    tool_cp_fg = "tool-cp-fg": [0x6B4A1FFF, 0xE6CDB0FF, 0xE6CDB0FF, 0xE6CDB0FF, 0xE6CDB0FF, 0xE6CDB0FF, 0xE6CDB0FF, 0xE6CDB0FF, 0xE6CDB0FF, 0xE6CDB0FF, 0xE6CDB0FF, 0x6B4A1FFF, 0x6B4A1FFF, 0x6B4A1FFF, 0x6B4A1FFF];
    tool_oc = "tool-oc": [0xE6E6E8FF, 0x3A3A3DFF, 0x3A3A3DFF, 0x3A3A3DFF, 0x3A3A3DFF, 0x3A3A3DFF, 0x3A3A3DFF, 0x3A3A3DFF, 0x3A3A3DFF, 0x3A3A3DFF, 0x3A3A3DFF, 0xE6E6E8FF, 0xE6E6E8FF, 0xE6E6E8FF, 0xE6E6E8FF];
    tool_oc_fg = "tool-oc-fg": [0x3D3D42FF, 0xD6D6DAFF, 0xD6D6DAFF, 0xD6D6DAFF, 0xD6D6DAFF, 0xD6D6DAFF, 0xD6D6DAFF, 0xD6D6DAFF, 0xD6D6DAFF, 0xD6D6DAFF, 0xD6D6DAFF, 0x3D3D42FF, 0x3D3D42FF, 0x3D3D42FF, 0x3D3D42FF];
}

/// The 16 terminal colours (`ansi-0` … `ansi-15`) as RGB, one row per colour, one column per theme (same order as the
/// tokens).
#[rustfmt::skip]
const ANSI: [[u32; 15]; 16] = [
    [0x2A2C31, 0x1C1E23, 0x51576D, 0x494D64, 0x45475A, 0x15161E, 0x21222C, 0x3B4252, 0x282828, 0x26233A, 0x3F4451, 0x5C5F77, 0x24292F, 0x073642, 0xF2E9E1],
    [0xCC2F3B, 0xFF6670, 0xE78284, 0xED8796, 0xF38BA8, 0xF7768E, 0xFF5555, 0xBF616A, 0xCC241D, 0xEB6F92, 0xE05561, 0xD20F39, 0xCF222E, 0xDC322F, 0xB4637A],
    [0x25854A, 0x4CC77A, 0xA6D189, 0xA6DA95, 0xA6E3A1, 0x9ECE6A, 0x50FA7B, 0xA3BE8C, 0x98971A, 0x31748F, 0x8CC265, 0x40A02B, 0x116329, 0x859900, 0x286983],
    [0x9A6600, 0xF2BE4A, 0xE5C890, 0xEED49F, 0xF9E2AF, 0xE0AF68, 0xF1FA8C, 0xEBCB8B, 0xD79921, 0xF6C177, 0xD18F52, 0xDF8E1D, 0x4D2D00, 0xB58900, 0xEA9D34],
    [0x2C6AD6, 0x6AA6FF, 0x8CAAEE, 0x8AADF4, 0x89B4FA, 0x7AA2F7, 0xBD93F9, 0x81A1C1, 0x458588, 0x9CCFD8, 0x4AA5F0, 0x1E66F5, 0x0969DA, 0x268BD2, 0x56949F],
    [0x7650D2, 0xB196FF, 0xF4B8E4, 0xF5BDE6, 0xF5C2E7, 0xBB9AF7, 0xFF79C6, 0xB48EAD, 0xB16286, 0xC4A7E7, 0xC162DE, 0xEA76CB, 0x8250DF, 0xD33682, 0x907AA9],
    [0x0B8478, 0x3CD3C0, 0x81C8BE, 0x8BD5CA, 0x94E2D5, 0x7DCFFF, 0x8BE9FD, 0x88C0D0, 0x689D6A, 0xEBBCBA, 0x42B3C2, 0x179299, 0x1B7C83, 0x2AA198, 0xD7827E],
    [0x9A9DA4, 0xD6D8DD, 0xB5BFE2, 0xB8C0E0, 0xBAC2DE, 0xA9B1D6, 0xF8F8F2, 0xE5E9F0, 0xA89984, 0xE0DEF4, 0xD7DAE0, 0xACB0BE, 0x6E7781, 0xEEE8D5, 0x575279],
    [0x5B5F67, 0x5C6069, 0x626880, 0x5B6078, 0x585B70, 0x414868, 0x6272A4, 0x4C566A, 0x928374, 0x6E6A86, 0x4F5666, 0x6C6F85, 0x57606A, 0x002B36, 0x9893A5],
    [0xE0434F, 0xFF8A8F, 0xE78284, 0xED8796, 0xF38BA8, 0xF7768E, 0xFF6E6E, 0xBF616A, 0xFB4934, 0xEB6F92, 0xFF616E, 0xD20F39, 0xA40E26, 0xCB4B16, 0xB4637A],
    [0x2E9E59, 0x6FDB96, 0xA6D189, 0xA6DA95, 0xA6E3A1, 0x9ECE6A, 0x69FF94, 0xA3BE8C, 0xB8BB26, 0x31748F, 0xA5E075, 0x40A02B, 0x1A7F37, 0x586E75, 0x286983],
    [0xB57A00, 0xFFD375, 0xE5C890, 0xEED49F, 0xF9E2AF, 0xE0AF68, 0xFFFFA5, 0xEBCB8B, 0xFABD2F, 0xF6C177, 0xF0A45D, 0xDF8E1D, 0x633C01, 0x657B83, 0xEA9D34],
    [0x4C84E6, 0x8FBCFF, 0x8CAAEE, 0x8AADF4, 0x89B4FA, 0x7AA2F7, 0xD6ACFF, 0x81A1C1, 0x83A598, 0x9CCFD8, 0x4DC4FF, 0x1E66F5, 0x218BFF, 0x839496, 0x56949F],
    [0x8D6AE0, 0xC7B3FF, 0xF4B8E4, 0xF5BDE6, 0xF5C2E7, 0xBB9AF7, 0xFF92DF, 0xB48EAD, 0xD3869B, 0xC4A7E7, 0xDE73FF, 0xEA76CB, 0xA475F9, 0x6C71C4, 0x907AA9],
    [0x13A090, 0x6BE3D3, 0x81C8BE, 0x8BD5CA, 0x94E2D5, 0x7DCFFF, 0xA4FFFF, 0x8FBCBB, 0x8EC07C, 0xEBBCBA, 0x4CD1E0, 0x179299, 0x3192AA, 0x93A1A1, 0xD7827E],
    [0xC9CBD0, 0xFFFFFF, 0xA5ADCE, 0xA5ADCB, 0xA6ADC8, 0xC0CAF5, 0xFFFFFF, 0xECEFF4, 0xEBDBB2, 0xE0DEF4, 0xE6E6E6, 0xBCC0CC, 0x8C959F, 0xFDF6E3, 0x575279],
];

/// A theme's 16 terminal colours as RGB: normal 0–7, bright 8–15.
pub fn ansi(theme: ThemeId) -> [u32; 16] {
    ANSI.map(|row| row[theme as usize])
}

/// The bundled fonts (*Type · Spacing · Icons*): Geist for the UI, Geist Mono for code, terminal, IDs and numbers.
pub const FONT_UI: &str = "Geist";
pub const FONT_MONO: &str = "Geist Mono";

/// The 4 px grid: `s-1` … `s-6`.
pub mod space {
    pub const S1: f32 = 4.;
    pub const S2: f32 = 8.;
    pub const S3: f32 = 12.;
    pub const S4: f32 = 16.;
    pub const S5: f32 = 24.;
    pub const S6: f32 = 32.;
}

/// Radius: 4 for pills and inputs, 6 for buttons and cards, 8–10 for panels and dialogs.
pub mod radius {
    pub const SM: f32 = 4.;
    pub const MD: f32 = 6.;
    pub const LG: f32 = 8.;
    pub const DIALOG: f32 = 10.;
}

/// Density: column width, card padding, header and list row heights.
pub mod density {
    /// A column's minimum: six open columns fit a 1440 window (~219 px each, *03 · Width strategy*, d0066).
    pub const COLUMN: f32 = 216.;
    pub const COLUMN_COMPACT: f32 = 184.;
    /// Card padding: 10 vertical × 12 horizontal.
    pub const CARD_PADDING: (f32, f32) = (10., 12.);
    pub const HEADER: f32 = 44.;
    pub const ROW: f32 = 28.;
}

/// One step of the type scale: size and letter spacing in px, weight, line height as a multiple of the size
/// (`None`: the font's own), and whether it is set in [`FONT_MONO`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub weight: u16,
    pub line_height: Option<f32>,
    pub letter_spacing: f32,
    pub mono: bool,
}

/// The type scale (*Type · Spacing · Icons → Type scale*), by the design's names.
pub mod text {
    use super::TextStyle;

    const fn ui(size: f32, weight: u16, line_height: Option<f32>) -> TextStyle {
        TextStyle {
            size,
            weight,
            line_height,
            letter_spacing: 0.,
            mono: false,
        }
    }

    const fn mono(size: f32, line_height: Option<f32>) -> TextStyle {
        TextStyle {
            size,
            weight: 400,
            line_height,
            letter_spacing: 0.,
            mono: true,
        }
    }

    /// Display and dialog titles: 20 / 600 / -0.3.
    pub const DISPLAY: TextStyle = TextStyle {
        letter_spacing: -0.3,
        ..ui(20., 600, None)
    };
    /// The header's project name: 15 / 600.
    pub const HEADER: TextStyle = ui(15., 600, None);
    /// Column titles: 12 / 600.
    pub const COLUMN_TITLE: TextStyle = ui(12., 600, None);
    /// Card titles: 13 / 500 / 1.35.
    pub const CARD_TITLE: TextStyle = ui(13., 500, Some(1.35));
    /// Body text: 13 / 400 / 1.5.
    pub const BODY: TextStyle = ui(13., 400, Some(1.5));
    /// Metadata: 11 / 400.
    pub const META: TextStyle = ui(11., 400, None);
    /// Code and IDs: 12 mono.
    pub const CODE: TextStyle = mono(12., None);
    /// Terminal: 13 mono / 1.45.
    pub const TERMINAL: TextStyle = mono(13., Some(1.45));

    /// Every step with its name, for UIs that list them (the page's CSS variables `--text-<name>-…`).
    pub const ALL: [(&str, TextStyle); 8] = [
        ("display", DISPLAY),
        ("header", HEADER),
        ("column-title", COLUMN_TITLE),
        ("card-title", CARD_TITLE),
        ("body", BODY),
        ("meta", META),
        ("code", CODE),
        ("terminal", TERMINAL),
    ];
}

/// How loudly a status asks for attention (*Status Legend*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Loud: turquoise = blocked on you; red = broke and waits for your click.
    NeedsYou,
    /// Medium: your turn, but nothing is stuck.
    Review,
    /// Calm.
    Working,
    /// Fades back: finished or not started.
    Done,
}

/// Which pair of status colours a status uses. `PillPalette` on the page, where each picks its two variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "PillPalette")]
pub enum Palette {
    Attn,
    Error,
    Review,
    Paused,
    Working,
    Queued,
    Approved,
    Done,
    Pinned,
}

/// Every entry of the *Status Legend*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    NeedsYou,
    AsksYou,
    Idle,
    Interrupted,
    StartInterrupted,
    ParentClosed,
    ReadyForReview,
    PrOpen,
    Conflicts,
    ChangedSinceReview,
    AgentWorking,
    WorkingInBackground,
    Queued,
    Parked,
    Running,
    Asked,
    SentBack,
    WaitingForLimitReset,
    Backlog,
    Approved,
    Done,
    Merged,
    ClosedNotMerged,
    Pinned,
    // M12 (d0063, d0064, d0066): *Status Legend → Pills · M12*, in [`Status::M12`].
    ValidatorNeedsYou,
    ValidationFailed,
    ValidationNoResult,
    AutoMergeStopped,
    Validated,
    ValidationSkipped,
    SubmittedByYou,
    FailedMoved,
    Validating,
    ValidationRound,
    MergeConflictFixing,
    MergedAutomatically,
    ReadyToStart,
    ProjectAgent,
}

impl Status {
    pub const ALL: [Status; 24] = [
        Self::NeedsYou,
        Self::AsksYou,
        Self::Idle,
        Self::Interrupted,
        Self::StartInterrupted,
        Self::ParentClosed,
        Self::ReadyForReview,
        Self::PrOpen,
        Self::Conflicts,
        Self::ChangedSinceReview,
        Self::AgentWorking,
        Self::WorkingInBackground,
        Self::Queued,
        Self::Parked,
        Self::Running,
        Self::Asked,
        Self::SentBack,
        Self::WaitingForLimitReset,
        Self::Backlog,
        Self::Approved,
        Self::Done,
        Self::Merged,
        Self::ClosedNotMerged,
        Self::Pinned,
    ];

    /// The legend's M12 row of each tier (*Pills · M12 (0063–0066)*), in the frame's order. "Waiting for a slot ·
    /// 5 / 5" is the Queued entry with its live label, so it isn't listed here.
    pub const M12: [Status; 14] = [
        Self::ValidatorNeedsYou,
        Self::ValidationFailed,
        Self::ValidationNoResult,
        Self::AutoMergeStopped,
        Self::Validated,
        Self::ValidationSkipped,
        Self::SubmittedByYou,
        Self::FailedMoved,
        Self::Validating,
        Self::ValidationRound,
        Self::MergeConflictFixing,
        Self::MergedAutomatically,
        Self::ReadyToStart,
        Self::ProjectAgent,
    ];

    /// Tier, palette and icon, as in the legend.
    pub fn spec(self) -> (Tier, Palette, IconName) {
        use IconName as I;
        use Palette as P;
        use Tier as T;
        match self {
            Self::NeedsYou => (T::NeedsYou, P::Attn, I::Hand),
            Self::AsksYou => (T::NeedsYou, P::Attn, I::MessageCircleQuestionMark),
            Self::Idle => (T::NeedsYou, P::Attn, I::CirclePause),
            Self::Interrupted => (T::NeedsYou, P::Error, I::ZapOff),
            Self::StartInterrupted => (T::NeedsYou, P::Error, I::TriangleAlert),
            Self::ParentClosed => (T::NeedsYou, P::Error, I::CircleSlash),
            Self::ReadyForReview => (T::Review, P::Review, I::Eye),
            Self::PrOpen => (T::Review, P::Review, I::GitPullRequest),
            Self::Conflicts => (T::Review, P::Paused, I::GitCompareArrows),
            Self::ChangedSinceReview => (T::Review, P::Paused, I::GitCommitHorizontal),
            Self::AgentWorking => (T::Working, P::Working, I::Activity),
            Self::WorkingInBackground => (T::Working, P::Working, I::Cpu),
            Self::Queued => (T::Working, P::Queued, I::Hourglass),
            Self::Parked => (T::Working, P::Paused, I::SquareParking),
            Self::Running => (T::Working, P::Approved, I::Play),
            Self::Asked => (T::Working, P::Working, I::MessagesSquare),
            Self::SentBack => (T::Working, P::Paused, I::Undo2),
            Self::WaitingForLimitReset => (T::Working, P::Queued, I::Hourglass),
            Self::Backlog => (T::Done, P::Queued, I::CircleDashed),
            Self::Approved => (T::Done, P::Approved, I::Check),
            Self::Done => (T::Done, P::Done, I::CircleCheck),
            Self::Merged => (T::Done, P::Done, I::GitMerge),
            Self::ClosedNotMerged => (T::Done, P::Done, I::CircleSlash),
            Self::Pinned => (T::Done, P::Pinned, I::Pin),
            Self::ValidatorNeedsYou => (T::NeedsYou, P::Attn, I::Hand),
            Self::ValidationFailed => (T::NeedsYou, P::Attn, I::ShieldX),
            Self::ValidationNoResult => (T::NeedsYou, P::Attn, I::ShieldQuestionMark),
            Self::AutoMergeStopped => (T::NeedsYou, P::Attn, I::GitCompareArrows),
            Self::Validated => (T::Review, P::Review, I::ShieldCheck),
            Self::ValidationSkipped => (T::Review, P::Queued, I::ShieldOff),
            Self::SubmittedByYou => (T::Review, P::Queued, I::UserCheck),
            // Red: did not pass.
            Self::FailedMoved => (T::Review, P::Error, I::ShieldX),
            Self::Validating => (T::Working, P::Working, I::ShieldEllipsis),
            Self::ValidationRound => (T::Working, P::Paused, I::ShieldAlert),
            Self::MergeConflictFixing => (T::Working, P::Paused, I::GitCompareArrows),
            Self::MergedAutomatically => (T::Done, P::Done, I::GitMerge),
            Self::ReadyToStart => (T::Done, P::Queued, I::CirclePlay),
            Self::ProjectAgent => (T::Done, P::Queued, I::Bot),
        }
    }

    /// The legend's label. Labels with live parts ("Serving :4020", "Asked SHOP-41", "Conflicts with main") are completed by the views that know those values.
    pub fn label(self) -> &'static str {
        match self {
            Self::NeedsYou => "Needs you",
            Self::AsksYou => "Asks you",
            Self::Idle => "Idle",
            Self::Interrupted => "Interrupted",
            Self::StartInterrupted => "Start failed",
            Self::ParentClosed => "Parent closed",
            Self::ReadyForReview => "Ready for Review",
            Self::PrOpen => "PR open",
            Self::Conflicts => "Conflicts with",
            Self::ChangedSinceReview => "Changed since review",
            Self::AgentWorking => "Working",
            Self::WorkingInBackground => "Working in background",
            Self::Queued => "Ready for Dev",
            Self::Parked => "Parked · has work",
            Self::Running => "Serving",
            Self::Asked => "Asked",
            Self::SentBack => "Sent back",
            Self::WaitingForLimitReset => "Waiting for limit reset",
            Self::Backlog => "Backlog",
            Self::Approved => "Approved",
            Self::Done => "Done",
            Self::Merged => "Merged",
            Self::ClosedNotMerged => "Closed · not merged",
            Self::Pinned => "Pinned",
            Self::ValidatorNeedsYou => otaq_core::domain::validation::VALIDATOR_NEEDS_YOU,
            Self::ValidationFailed => "Validation failed",
            Self::ValidationNoResult => otaq_core::domain::validation::NO_RESULT,
            Self::AutoMergeStopped => "Delivery stopped",
            Self::Validated => "Validated",
            Self::ValidationSkipped => "Validation skipped by you",
            Self::SubmittedByYou => "Submitted by you",
            Self::FailedMoved => "Failed · moved by you",
            Self::Validating => otaq_core::domain::validation::VALIDATING,
            Self::ValidationRound => "Validation failed · round",
            Self::MergeConflictFixing => otaq_core::domain::validation::MERGE_CONFLICT_FIXING,
            Self::MergedAutomatically => otaq_core::domain::validation::MERGED_AUTOMATICALLY,
            Self::ReadyToStart => "Ready to start",
            Self::ProjectAgent => "by Noyan",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The service answers a program's colour questions with the core's copy of `term-fg` / `term-bg` (d0117 point
    /// 4): it must be the design's, theme by theme, and the kind it tells agents must be the theme's appearance.
    #[test]
    fn the_cores_terminal_colours_are_the_tokens() {
        for theme in ThemeId::ALL {
            let colors = Colors::new(theme);
            let core = theme.stored().terminal_colors();
            assert_eq!(core.fg, colors.term_fg >> 8, "{theme:?} term-fg");
            assert_eq!(core.bg, colors.term_bg >> 8, "{theme:?} term-bg");
            assert_eq!(
                theme.stored().kind() == otaq_core::board::settings::ThemeKind::Dark,
                theme.appearance() == Appearance::Dark,
                "{theme:?}"
            );
        }
    }

    fn hex(s: &str) -> u32 {
        let v = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap();
        if s.len() == 7 { (v << 8) | 0xFF } else { v }
    }

    #[test]
    fn every_token_resolves_in_every_theme() {
        assert_eq!(Colors::<u32>::NAMES.len(), 49);
        for id in ThemeId::ALL {
            let colors = Colors::new(id);
            for name in Colors::<u32>::NAMES {
                assert!(colors.get(name).is_some(), "{name}");
            }
        }
        assert_eq!(Colors::new(ThemeId::Nord).get("no-such-token"), None);
    }

    /// Every colour variable of *crewboard.pen* (`GetVariables`, theme axes `palette` and, for Otaq, `mode`),
    /// copied as the design writes them: the 49 tokens then `ansi-0` … `ansi-15`, one column per theme in the order
    /// of `ThemeId::ALL`. Changing a value here means the design changed; changing it only in `color_tokens!` or
    /// `ANSI` is drift, and fails this test.
    #[rustfmt::skip]
    const DESIGN: &[(&str, [&str; 15])] = &[
        ("bg-app", ["#F6F6F4", "#0E0F11", "#232634", "#181926", "#11111b", "#16161e", "#191a21", "#242933", "#1d2021", "#13111d", "#1b1d23", "#e6e9ef", "#f6f8fa", "#f7f0dd", "#faf4ed"]),
        ("bg-panel", ["#FFFFFF", "#15161A", "#303446", "#24273a", "#1e1e2e", "#1a1b26", "#282a36", "#2e3440", "#282828", "#1f1d2e", "#282c34", "#eff1f5", "#ffffff", "#fdf6e3", "#fffaf3"]),
        ("bg-column", ["#EEEEEB", "#121316", "#292c3c", "#1e2030", "#181825", "#191a24", "#21222c", "#292e39", "#252423", "#191724", "#21252b", "#dce0e8", "#eff2f5", "#eee8d5", "#f2e9e1"]),
        ("bg-card", ["#FFFFFF", "#1C1E23", "#303446", "#24273a", "#1e1e2e", "#1f2335", "#2f3141", "#333946", "#32302f", "#26233a", "#2c313c", "#eff1f5", "#ffffff", "#fdf6e3", "#fffaf3"]),
        ("bg-raised", ["#FFFFFF", "#23262C", "#414559", "#363a4f", "#313244", "#24283b", "#343746", "#3b4252", "#3c3836", "#26233a", "#323842", "#eff1f5", "#ffffff", "#fdf6e3", "#fffaf3"]),
        ("bg-hover", ["#EDEDEA", "#262930", "#414559", "#363a4f", "#313244", "#292e42", "#3a3d4e", "#434c5e", "#3c3836", "#2a2740", "#353b45", "#e6e9ef", "#f3f5f7", "#f5efdc", "#f4ede8"]),
        ("bg-pressed", ["#E4E4E0", "#2E323A", "#51576d", "#494d64", "#45475a", "#343a55", "#44475a", "#4c566a", "#504945", "#403d52", "#3e4451", "#dce0e8", "#eaeef2", "#eee8d5", "#efe6de"]),
        ("border", ["#E2E2DE", "#272A31", "#414559", "#363a4f", "#313244", "#292e42", "#343746", "#3b4252", "#3c3836", "#2a2740", "#3a3f4b", "#ccd0da", "#d0d7de", "#e4ddc8", "#dfdad9"]),
        ("border-strong", ["#959588", "#686F7F", "#51576d", "#494d64", "#45475a", "#3b4261", "#565a70", "#4c566a", "#504945", "#403d52", "#4b5263", "#bcc0cc", "#afb8c1", "#d3cbb3", "#cecacd"]),
        ("border-card", ["#8A8A7A", "#62697B", "#414559", "#363a4f", "#313244", "#292e42", "#343746", "#3b4252", "#3c3836", "#2a2740", "#3a3f4b", "#ccd0da", "#d0d7de", "#e4ddc8", "#dfdad9"]),
        ("text-primary", ["#1A1B1E", "#ECEDEF", "#c6d0f5", "#cad3f5", "#cdd6f4", "#c0caf5", "#f8f8f2", "#eceff4", "#ebdbb2", "#e0def4", "#d7dae0", "#4c4f69", "#1f2328", "#586e75", "#575279"]),
        ("text-secondary", ["#5B5F67", "#A5A9B2", "#a5adce", "#a5adcb", "#a6adc8", "#a9b1d6", "#c3c5d4", "#d8dee9", "#bdae93", "#908caa", "#abb2bf", "#6c6f85", "#59636e", "#657b83", "#797593"]),
        ("text-muted", ["#676B74", "#8C9099", "#838ba7", "#8087a2", "#7f849c", "#737aa2", "#7b86b5", "#7b88a1", "#928374", "#6e6a86", "#7f848e", "#8c8fa1", "#818b98", "#93a1a1", "#9893a5"]),
        ("text-on-accent", ["#FFFFFF", "#0E0F11", "#232634", "#181926", "#11111b", "#1a1b26", "#282a36", "#2e3440", "#282828", "#191724", "#282c34", "#eff1f5", "#ffffff", "#fdf6e3", "#fffaf3"]),
        ("accent", ["#4C57D6", "#757FF7", "#ca9ee6", "#c6a0f6", "#cba6f7", "#7aa2f7", "#bd93f9", "#88c0d0", "#83a598", "#c4a7e7", "#61afef", "#8839ef", "#0969da", "#268bd2", "#907aa9"]),
        ("accent-hover", ["#3F49C2", "#828BFF", "#D2ADEA", "#CFAEF7", "#D3B3F8", "#8EB0F8", "#C7A3FA", "#9AC9D7", "#96B3A7", "#CDB4EB", "#79BBF1", "#7430CB", "#0859B9", "#2076B3", "#7A6890"]),
        ("accent-soft", ["#ECEEFD", "#1E2142", "#4C4763", "#413D5C", "#3D3652", "#2B334C", "#433D59", "#3E4D5A", "#383F3C", "#3D364F", "#324456", "#E5DFF4", "#E6F0FB", "#E8EBE1", "#F4EDEC"]),
        ("focus", ["#4C57D6", "#8A93FF", "#ca9ee6", "#c6a0f6", "#cba6f7", "#7aa2f7", "#bd93f9", "#88c0d0", "#83a598", "#c4a7e7", "#61afef", "#8839ef", "#0969da", "#268bd2", "#907aa9"]),
        ("selection", ["#4C57D633", "#6E78F74D", "#ca9ee64D", "#c6a0f64D", "#cba6f74D", "#7aa2f74D", "#bd93f94D", "#88c0d04D", "#83a5984D", "#c4a7e74D", "#61afef4D", "#8839ef33", "#0969da33", "#268bd233", "#907aa933"]),
        ("st-attn", ["#087A6F", "#2DD4BF", "#81c8be", "#8bd5ca", "#94e2d5", "#73daca", "#40e0d0", "#8fbcbb", "#8ec07c", "#9ccfd8", "#56b6c2", "#179299", "#1b7c83", "#2aa198", "#56949f"]),
        ("st-attn-bg", ["#E0F5F2", "#0E2E2A", "#3F4F5C", "#374654", "#33414C", "#2A3D44", "#2C4B52", "#3F4C56", "#3A4337", "#363D4D", "#30454E", "#D9E8EC", "#E8F2F3", "#E8EEDC", "#EEF0EB"]),
        ("on-attn", ["#FFFFFF", "#0E0F11", "#232634", "#181926", "#11111b", "#1a1b26", "#282a36", "#2e3440", "#282828", "#191724", "#282c34", "#eff1f5", "#ffffff", "#fdf6e3", "#fffaf3"]),
        ("st-error", ["#CC2F3B", "#FF6670", "#e78284", "#ed8796", "#f38ba8", "#f7768e", "#ff5555", "#bf616a", "#fb4934", "#eb6f92", "#e06c75", "#d20f39", "#d1242f", "#dc322f", "#b4637a"]),
        ("st-error-bg", ["#FDEBED", "#3A191C", "#514251", "#48384B", "#443244", "#422B39", "#4F323C", "#483C48", "#4E2E2A", "#442C40", "#493840", "#ECD8E0", "#FAE7E8", "#F9E0CF", "#F7E9E6"]),
        ("st-working", ["#2A68D6", "#6AA6FF", "#8caaee", "#8aadf4", "#89b4fa", "#7dcfff", "#8be9fd", "#81a1c1", "#83a598", "#3e8fb0", "#61afef", "#1e66f5", "#0969da", "#268bd2", "#286983"]),
        ("st-working-bg", ["#EAF1FD", "#15233B", "#414964", "#363F5B", "#313953", "#2C3B4D", "#3A4C5A", "#3D4857", "#383F3C", "#253245", "#324456", "#D8E2F5", "#E4EFFB", "#E5EAE1", "#E7EAE7"]),
        ("st-investigate", ["#7650D2", "#B196FF", "#babbf1", "#b7bdf8", "#b4befe", "#bb9af7", "#d6acff", "#b48ead", "#a89adb", "#c4a7e7", "#c678dd", "#8839ef", "#8250df", "#6c71c4", "#907aa9"]),
        ("st-investigate-bg", ["#F2EEFC", "#251C3B", "#494C65", "#3E425C", "#393B53", "#37324C", "#47415A", "#464454", "#3F3D48", "#3D364F", "#443A52", "#E5DFF4", "#F1ECFB", "#EDE7E0", "#F3ECEB"]),
        ("st-queued", ["#646A75", "#9AA0AB", "#949cbb", "#939ab7", "#9399b2", "#828bb8", "#9aa3c4", "#8e9aaf", "#a89984", "#908caa", "#9da5b4", "#7c7f93", "#59636e", "#839496", "#797593"]),
        ("st-queued-bg", ["#EFF0F2", "#23262C", "#3E4356", "#34374C", "#2E2F40", "#292B3A", "#383B4A", "#3B4250", "#3A3835", "#2F2D3F", "#383D46", "#E4E6EB", "#EEEFF1", "#F1ECDB", "#F2EDE9"]),
        ("st-review", ["#BA3888", "#F07CC4", "#f4b8e4", "#f5bde6", "#f5c2e7", "#f77fd6", "#ff79c6", "#dc94bd", "#d3869b", "#ec9bd1", "#e77fc0", "#ea76cb", "#bf3989", "#d33682", "#b85fa8"]),
        ("st-review-bg", ["#FBEAF4", "#3A1830", "#534C62", "#4A4259", "#453C4F", "#422D46", "#4F3850", "#4D4557", "#47393D", "#44344B", "#4A3B4D", "#EFE5F1", "#F9EBF3", "#F9E3D9", "#F8EBEC"]),
        ("st-paused", ["#976400", "#F2BE4A", "#e5c890", "#eed49f", "#f9e2af", "#e0af68", "#f1fa8c", "#ebcb8b", "#fabd2f", "#ebbcba", "#e5c07b", "#df8e1d", "#9a6700", "#b58900", "#d7827e"]),
        ("st-paused-bg", ["#FAF2DA", "#33290E", "#514F53", "#48464C", "#454145", "#3E3632", "#4C4F45", "#504F4E", "#4E4329", "#443A47", "#4A4741", "#EDE6DD", "#F4EEE3", "#F5EACA", "#FBEDE6"]),
        ("st-approved", ["#237D46", "#4CC77A", "#a6d189", "#a6da95", "#a6e3a1", "#9ece6a", "#50fa7b", "#a3be8c", "#b8bb26", "#a3c9a8", "#98c379", "#40a02b", "#1a7f37", "#859900", "#5f8f6a"]),
        ("st-approved-bg", ["#E6F4EA", "#12301C", "#455052", "#3B474A", "#364143", "#323B32", "#2F4F42", "#434D4E", "#424228", "#373C44", "#3C4740", "#DCE8DF", "#E6F1E9", "#F0ECCA", "#EDEEE4"]),
        ("st-done", ["#868992", "#6A6F79", "#737994", "#6e738d", "#6c7086", "#565f89", "#6272a4", "#616e88", "#7c6f64", "#524f67", "#5c6370", "#9ca0b0", "#8c959f", "#93a1a1", "#9893a5"]),
        ("term-bg", ["#FAFAF8", "#0A0B0D", "#303446", "#24273a", "#1e1e2e", "#1a1b26", "#282a36", "#2e3440", "#282828", "#191724", "#282c34", "#eff1f5", "#f6f8fa", "#fdf6e3", "#faf4ed"]),
        ("term-fg", ["#2A2C31", "#D6D8DD", "#c6d0f5", "#cad3f5", "#cdd6f4", "#c0caf5", "#f8f8f2", "#d8dee9", "#ebdbb2", "#e0def4", "#abb2bf", "#4c4f69", "#1f2328", "#657b83", "#575279"]),
        ("diff-add-bg", ["#E5F6EA", "#11291A", "#434D51", "#394449", "#343E40", "#2F3831", "#2E4B41", "#414A4C", "#3F4028", "#2F3339", "#3A443F", "#DAE7DD", "#DCE9E3", "#EFEBC8", "#E7E8DD"]),
        ("diff-del-bg", ["#FCEBEC", "#2E1517", "#4D4050", "#443649", "#402F42", "#3D2A37", "#4A313B", "#453B47", "#4A2D2A", "#3B2536", "#45363E", "#ECD6DE", "#F2DFE2", "#F9DECD", "#F2E3DF"]),
        ("tool-cc", ["#E4E9CF", "#3A4220", "#3A4220", "#3A4220", "#3A4220", "#3A4220", "#3A4220", "#3A4220", "#3A4220", "#3A4220", "#3A4220", "#E4E9CF", "#E4E9CF", "#E4E9CF", "#E4E9CF"]),
        ("tool-cc-fg", ["#4A5A12", "#D3DFA0", "#D3DFA0", "#D3DFA0", "#D3DFA0", "#D3DFA0", "#D3DFA0", "#D3DFA0", "#D3DFA0", "#D3DFA0", "#D3DFA0", "#4A5A12", "#4A5A12", "#4A5A12", "#4A5A12"]),
        ("tool-cx", ["#DCE3EC", "#2B3647", "#2B3647", "#2B3647", "#2B3647", "#2B3647", "#2B3647", "#2B3647", "#2B3647", "#2B3647", "#2B3647", "#DCE3EC", "#DCE3EC", "#DCE3EC", "#DCE3EC"]),
        ("tool-cx-fg", ["#2E405A", "#BFD0E8", "#BFD0E8", "#BFD0E8", "#BFD0E8", "#BFD0E8", "#BFD0E8", "#BFD0E8", "#BFD0E8", "#BFD0E8", "#BFD0E8", "#2E405A", "#2E405A", "#2E405A", "#2E405A"]),
        ("tool-cp", ["#EDE3D3", "#43352A", "#43352A", "#43352A", "#43352A", "#43352A", "#43352A", "#43352A", "#43352A", "#43352A", "#43352A", "#EDE3D3", "#EDE3D3", "#EDE3D3", "#EDE3D3"]),
        ("tool-cp-fg", ["#6B4A1F", "#E6CDB0", "#E6CDB0", "#E6CDB0", "#E6CDB0", "#E6CDB0", "#E6CDB0", "#E6CDB0", "#E6CDB0", "#E6CDB0", "#E6CDB0", "#6B4A1F", "#6B4A1F", "#6B4A1F", "#6B4A1F"]),
        ("tool-oc", ["#E6E6E8", "#3A3A3D", "#3A3A3D", "#3A3A3D", "#3A3A3D", "#3A3A3D", "#3A3A3D", "#3A3A3D", "#3A3A3D", "#3A3A3D", "#3A3A3D", "#E6E6E8", "#E6E6E8", "#E6E6E8", "#E6E6E8"]),
        ("tool-oc-fg", ["#3D3D42", "#D6D6DA", "#D6D6DA", "#D6D6DA", "#D6D6DA", "#D6D6DA", "#D6D6DA", "#D6D6DA", "#D6D6DA", "#D6D6DA", "#D6D6DA", "#3D3D42", "#3D3D42", "#3D3D42", "#3D3D42"]),
        ("ansi-0", ["#2A2C31", "#1C1E23", "#51576d", "#494d64", "#45475a", "#15161e", "#21222c", "#3b4252", "#282828", "#26233a", "#3f4451", "#5c5f77", "#24292f", "#073642", "#f2e9e1"]),
        ("ansi-1", ["#CC2F3B", "#FF6670", "#e78284", "#ed8796", "#f38ba8", "#f7768e", "#ff5555", "#bf616a", "#cc241d", "#eb6f92", "#e05561", "#d20f39", "#cf222e", "#dc322f", "#b4637a"]),
        ("ansi-2", ["#25854A", "#4CC77A", "#a6d189", "#a6da95", "#a6e3a1", "#9ece6a", "#50fa7b", "#a3be8c", "#98971a", "#31748f", "#8cc265", "#40a02b", "#116329", "#859900", "#286983"]),
        ("ansi-3", ["#9A6600", "#F2BE4A", "#e5c890", "#eed49f", "#f9e2af", "#e0af68", "#f1fa8c", "#ebcb8b", "#d79921", "#f6c177", "#d18f52", "#df8e1d", "#4d2d00", "#b58900", "#ea9d34"]),
        ("ansi-4", ["#2C6AD6", "#6AA6FF", "#8caaee", "#8aadf4", "#89b4fa", "#7aa2f7", "#bd93f9", "#81a1c1", "#458588", "#9ccfd8", "#4aa5f0", "#1e66f5", "#0969da", "#268bd2", "#56949f"]),
        ("ansi-5", ["#7650D2", "#B196FF", "#f4b8e4", "#f5bde6", "#f5c2e7", "#bb9af7", "#ff79c6", "#b48ead", "#b16286", "#c4a7e7", "#c162de", "#ea76cb", "#8250df", "#d33682", "#907aa9"]),
        ("ansi-6", ["#0B8478", "#3CD3C0", "#81c8be", "#8bd5ca", "#94e2d5", "#7dcfff", "#8be9fd", "#88c0d0", "#689d6a", "#ebbcba", "#42b3c2", "#179299", "#1b7c83", "#2aa198", "#d7827e"]),
        ("ansi-7", ["#9A9DA4", "#D6D8DD", "#b5bfe2", "#b8c0e0", "#bac2de", "#a9b1d6", "#f8f8f2", "#e5e9f0", "#a89984", "#e0def4", "#d7dae0", "#acb0be", "#6e7781", "#eee8d5", "#575279"]),
        ("ansi-8", ["#5B5F67", "#5C6069", "#626880", "#5b6078", "#585b70", "#414868", "#6272a4", "#4c566a", "#928374", "#6e6a86", "#4f5666", "#6c6f85", "#57606a", "#002b36", "#9893a5"]),
        ("ansi-9", ["#E0434F", "#FF8A8F", "#e78284", "#ed8796", "#f38ba8", "#f7768e", "#ff6e6e", "#bf616a", "#fb4934", "#eb6f92", "#ff616e", "#d20f39", "#a40e26", "#cb4b16", "#b4637a"]),
        ("ansi-10", ["#2E9E59", "#6FDB96", "#a6d189", "#a6da95", "#a6e3a1", "#9ece6a", "#69ff94", "#a3be8c", "#b8bb26", "#31748f", "#a5e075", "#40a02b", "#1a7f37", "#586e75", "#286983"]),
        ("ansi-11", ["#B57A00", "#FFD375", "#e5c890", "#eed49f", "#f9e2af", "#e0af68", "#ffffa5", "#ebcb8b", "#fabd2f", "#f6c177", "#f0a45d", "#df8e1d", "#633c01", "#657b83", "#ea9d34"]),
        ("ansi-12", ["#4C84E6", "#8FBCFF", "#8caaee", "#8aadf4", "#89b4fa", "#7aa2f7", "#d6acff", "#81a1c1", "#83a598", "#9ccfd8", "#4dc4ff", "#1e66f5", "#218bff", "#839496", "#56949f"]),
        ("ansi-13", ["#8D6AE0", "#C7B3FF", "#f4b8e4", "#f5bde6", "#f5c2e7", "#bb9af7", "#ff92df", "#b48ead", "#d3869b", "#c4a7e7", "#de73ff", "#ea76cb", "#a475f9", "#6c71c4", "#907aa9"]),
        ("ansi-14", ["#13A090", "#6BE3D3", "#81c8be", "#8bd5ca", "#94e2d5", "#7dcfff", "#a4ffff", "#8fbcbb", "#8ec07c", "#ebbcba", "#4cd1e0", "#179299", "#3192aa", "#93a1a1", "#d7827e"]),
        ("ansi-15", ["#C9CBD0", "#FFFFFF", "#a5adce", "#a5adcb", "#a6adc8", "#c0caf5", "#ffffff", "#eceff4", "#ebdbb2", "#e0def4", "#e6e6e6", "#bcc0cc", "#8c959f", "#fdf6e3", "#575279"]),
    ];

    #[test]
    fn token_values_match_the_design() {
        for (index, id) in ThemeId::ALL.into_iter().enumerate() {
            let colors = Colors::new(id);
            let ansi = ansi(id);
            for (name, values) in DESIGN {
                let want = hex(values[index]);
                match name.strip_prefix("ansi-") {
                    Some(n) => {
                        let n: usize = n.parse().unwrap();
                        assert_eq!(ansi[n], want >> 8, "{name} ({:?})", id);
                    }
                    None => assert_eq!(colors.get(name), Some(want), "{name} ({:?})", id),
                }
            }
        }
        // The same tokens in the design's order, then the 16 terminal colours.
        let names: Vec<&str> = DESIGN.iter().map(|(name, _)| *name).collect();
        assert_eq!(&names[..49], Colors::<u32>::NAMES);
        let ansi_names: Vec<String> = (0..16).map(|n| format!("ansi-{n}")).collect();
        let ansi_names: Vec<&str> = ansi_names.iter().map(String::as_str).collect();
        assert_eq!(&names[49..], ansi_names.as_slice());
    }

    #[test]
    fn themes_are_the_bundled_fifteen() {
        assert_eq!(
            ThemeId::DARK.len() + ThemeId::LIGHT.len(),
            ThemeId::ALL.len()
        );
        for id in ThemeId::DARK {
            assert_eq!(id.appearance(), Appearance::Dark, "{id:?}");
            assert_eq!(ThemeId::from_stored(id.stored()), id);
        }
        for id in ThemeId::LIGHT {
            assert_eq!(id.appearance(), Appearance::Light, "{id:?}");
            assert_eq!(ThemeId::from_stored(id.stored()), id);
        }
        // The pickers list the themes in the stored order.
        let listed: Vec<StoredTheme> = ThemeId::DARK
            .into_iter()
            .chain(ThemeId::LIGHT)
            .map(ThemeId::stored)
            .collect();
        assert_eq!(listed, StoredTheme::ALL);
        assert_eq!(ThemeId::TokyoNight.stored().as_str(), "tokyo-night");
        assert_eq!(ThemeId::RosePineDawn.name(), "Rosé Pine Dawn");
        assert_eq!(
            ThemeId::OtaqDark.name(),
            format!("{} Dark", naming::PRODUCT_NAME)
        );
        assert_eq!(ansi(ThemeId::Dracula)[1], 0xFF5555);
    }

    #[test]
    fn a_picked_theme_ignores_the_os_and_an_unpicked_one_follows_it() {
        let picked = ThemeSetting::Picked(StoredTheme::Daylight);
        for os in [Appearance::Dark, Appearance::Light] {
            assert_eq!(ThemeId::from_setting(picked, os), ThemeId::Daylight);
        }
        let unpicked = ThemeSetting::default();
        assert_eq!(
            ThemeId::from_setting(unpicked, Appearance::Dark),
            ThemeId::OtaqDark
        );
        assert_eq!(
            ThemeId::from_setting(unpicked, Appearance::Light),
            ThemeId::OtaqLight
        );
    }
}
