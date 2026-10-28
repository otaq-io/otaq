//! Plugins (the otaq-marketplace README): installable ticket sources that Otaq itself runs. A plugin is a declarative
//! manifest naming the vendor CLIs it runs (`gh`, `acli`, `az`) and how their JSON output becomes issues; Otaq stores
//! no tokens (the CLIs' own logins) and passes no project environment variables.

pub mod bundle;
pub mod checks;
pub mod index;
pub mod manifest;
pub mod mapping;
pub mod markdown;
pub mod merge;
pub mod open_source;
pub mod params;
pub mod path;
pub mod registry;
pub mod runner;
pub mod template;
pub mod values;

pub use registry::Plugins;
