//! Anonymous usage data (d0085): the "Help improve Otaq?" banner, a quiet one-time banner after the first ticket is
//! approved, and the words of its row in Settings → Data. The desktop app decides when the banner shows and what is
//! sent; this only words them.

use serde::Serialize;
use ts_rs::TS;

use otaq_core::naming::PRODUCT_NAME;

/// The Settings → Data row's label.
pub const ROW_LABEL: &str = "Send anonymous usage data";

/// The Settings → Data row's description.
pub fn row_description() -> String {
    format!(
        "Which features are used and how often, with the {PRODUCT_NAME} version and the OS. Never ticket text, code, \
         names, paths or anything that identifies you. Off unless you turned it on."
    )
}

/// "Help improve Otaq?": a quiet one-time banner after the first ticket is approved. Nothing is sent unless the user
/// presses Share; either answer is stored, and Settings → Data changes it later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "UsageDataBanner")]
pub struct Banner {
    pub title: String,
    pub text: String,
    pub decline_label: String,
    pub accept_label: String,
}

/// The banner's words.
pub fn banner() -> Banner {
    Banner {
        title: format!("Help improve {PRODUCT_NAME}?"),
        text: "Share anonymous usage data: which screens and actions are used. Never code, tickets or names. You can \
               change it in Settings \u{2192} Data."
            .into(),
        decline_label: "No thanks".into(),
        accept_label: "Share".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_banner_names_the_product_and_offers_both_answers() {
        let b = banner();
        assert_eq!(b.title, format!("Help improve {PRODUCT_NAME}?"));
        assert_eq!(
            (b.decline_label.as_str(), b.accept_label.as_str()),
            ("No thanks", "Share")
        );
        assert!(b.text.contains("Never code, tickets or names"));
        assert!(row_description().contains("Off unless you turned it on"));
    }
}
