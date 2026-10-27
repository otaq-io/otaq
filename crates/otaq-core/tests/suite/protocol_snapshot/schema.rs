//! The protocol's wire shape, read from the crate's source with `syn`: every type reachable from
//! `protocol::Message`, with its fields and variants under their wire names (after `rename` / `rename_all`) and the
//! serde attributes that change the wire. Doc comments, field order and Rust-only names don't count.
//!
//! Small on purpose: it only has to be right for this repository. What it can't see is said in the snapshot test's
//! module comment.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use quote::ToTokens;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use syn::{Attribute, Fields, GenericArgument, Item, PathArguments, Token, Type};

/// What the snapshot records: the protocol version and every type on the wire, by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schema {
    pub protocol_version: u32,
    pub root: String,
    pub types: BTreeMap<String, Shape>,
}

/// One type's wire shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape {
    Struct {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attrs: Vec<String>,
        fields: BTreeMap<String, Field>,
    },
    /// A tuple struct (a newtype struct has one item).
    Tuple {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attrs: Vec<String>,
        items: Vec<String>,
    },
    Unit,
    Enum {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attrs: Vec<String>,
        variants: BTreeMap<String, Variant>,
    },
    /// `type X = Y;`
    Alias {
        ty: String,
    },
    /// A fieldless enum written as one string per variant (`stored_enum!`): the strings.
    Strings {
        values: BTreeSet<String>,
    },
    /// Hand-written `Serialize` / `Deserialize`: a fingerprint of the two impls.
    Custom {
        serde_impl: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub ty: String,
    /// A reader fails when it's missing (no `Option`, no `default`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
    /// A writer may leave it out (`skip_serializing_if`, `skip_serializing`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub omittable: bool,
    /// Anything else that changes the wire (`flatten`, `with`, `alias`, ...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variant {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newtype: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tuple: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<BTreeMap<String, Field>>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// A type expression as the snapshot writes it: `Option<Vec<Ticket>>`, `(u32, String)`, `[u8; 4]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ty {
    pub name: String,
    pub args: Vec<Ty>,
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let args = self.args.iter().map(Ty::to_string).collect::<Vec<_>>();
        match self.name.as_str() {
            "()" => write!(f, "({})", args.join(", ")),
            name if name.starts_with("[;") => {
                write!(f, "[{}; {}]", args[0], &name[2..name.len() - 1])
            }
            name if args.is_empty() => f.write_str(name),
            name => write!(f, "{name}<{}>", args.join(", ")),
        }
    }
}

impl Ty {
    fn leaf(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            args: Vec::new(),
        }
    }

    /// Reads back what `Display` wrote.
    pub fn parse(text: &str) -> Ty {
        let mut chars = text.chars().filter(|c| !c.is_whitespace()).peekable();
        let ty = parse_ty(&mut chars);
        assert!(
            chars.next().is_none(),
            "unreadable type in the snapshot: {text}"
        );
        ty
    }
}

fn parse_ty(chars: &mut std::iter::Peekable<impl Iterator<Item = char>>) -> Ty {
    match chars.peek() {
        Some('(') => {
            chars.next();
            let args = parse_list(chars, ')');
            Ty {
                name: "()".into(),
                args,
            }
        }
        Some('[') => {
            chars.next();
            let item = parse_ty(chars);
            assert_eq!(
                chars.next(),
                Some(';'),
                "unreadable array type in the snapshot"
            );
            let len: String = std::iter::from_fn(|| chars.next_if(|c| *c != ']')).collect();
            chars.next();
            Ty {
                name: format!("[;{len}]"),
                args: vec![item],
            }
        }
        _ => {
            let name: String = std::iter::from_fn(|| {
                chars.next_if(|c| !matches!(c, '<' | '>' | ',' | ')' | ']' | ';'))
            })
            .collect();
            let args = if chars.next_if_eq(&'<').is_some() {
                parse_list(chars, '>')
            } else {
                Vec::new()
            };
            Ty { name, args }
        }
    }
}

fn parse_list(chars: &mut std::iter::Peekable<impl Iterator<Item = char>>, close: char) -> Vec<Ty> {
    let mut items = Vec::new();
    loop {
        if chars.next_if_eq(&close).is_some() {
            return items;
        }
        items.push(parse_ty(chars));
        chars.next_if_eq(&',');
    }
}

// ---- Reading the source ----

/// A module's absolute path inside the crate (`[]` is the crate root).
type ModPath = Vec<String>;

/// A type definition: where it is and what it is.
#[derive(Clone)]
enum Def {
    Struct(syn::ItemStruct),
    Enum(syn::ItemEnum),
    Alias(syn::ItemType),
    Strings(BTreeSet<String>),
}

#[derive(Default)]
struct Module {
    defs: HashMap<String, Def>,
    /// `use a::b::C as D;` → `D` → `[a, b, C]` (as written, resolved from this module).
    uses: HashMap<String, Vec<String>>,
    globs: Vec<Vec<String>>,
    children: HashSet<String>,
    /// Hand-written serde impls, by the type's name: their tokens.
    serde_impls: HashMap<String, Vec<String>>,
}

/// Where a path leads.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Target {
    Local(ModPath, String),
    External(String),
}

pub struct Crate {
    modules: HashMap<ModPath, Module>,
}

impl Crate {
    /// Reads the crate from its `src/lib.rs`, following `mod` declarations (`#[cfg(test)]` ones skipped).
    pub fn read(src: &Path) -> Crate {
        let mut krate = Crate {
            modules: HashMap::new(),
        };
        krate.read_file(&src.join("lib.rs"), src.to_path_buf(), Vec::new());
        krate
    }

    /// Builds a crate from source text, one module per entry (for the checker's own tests).
    pub fn from_sources(sources: &[(&str, &str)]) -> Crate {
        let mut krate = Crate {
            modules: HashMap::new(),
        };
        for (path, text) in sources {
            let path: ModPath = path
                .split("::")
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
            let file = syn::parse_file(text).expect("test source parses");
            if let Some((last, parent)) = path.split_last() {
                krate
                    .modules
                    .entry(parent.to_vec())
                    .or_default()
                    .children
                    .insert(last.clone());
            }
            krate.read_items(&file.items, None, path);
        }
        krate
    }

    fn read_file(&mut self, file: &Path, dir: PathBuf, path: ModPath) {
        let text =
            fs::read_to_string(file).unwrap_or_else(|e| panic!("reading {}: {e}", file.display()));
        let parsed =
            syn::parse_file(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", file.display()));
        self.read_items(&parsed.items, Some(dir), path);
    }

    /// `dir` is where this module's child files live; `None` for source given as text.
    fn read_items(&mut self, items: &[Item], dir: Option<PathBuf>, path: ModPath) {
        self.modules.entry(path.clone()).or_default();
        for item in items {
            if is_test_only(item_attrs(item)) {
                continue;
            }
            match item {
                Item::Struct(s) => self.def(&path, s.ident.to_string(), Def::Struct(s.clone())),
                Item::Enum(e) => self.def(&path, e.ident.to_string(), Def::Enum(e.clone())),
                Item::Type(t) => self.def(&path, t.ident.to_string(), Def::Alias(t.clone())),
                Item::Use(u) => {
                    let mut prefix = Vec::new();
                    self.read_use(&path, &u.tree, &mut prefix);
                }
                Item::Mod(m) => {
                    let name = m.ident.to_string();
                    self.modules
                        .entry(path.clone())
                        .or_default()
                        .children
                        .insert(name.clone());
                    let mut child = path.clone();
                    child.push(name.clone());
                    match (&m.content, &dir) {
                        (Some((_, inner)), _) => {
                            let inner_dir = dir.as_ref().map(|d| d.join(&name));
                            self.read_items(inner, inner_dir, child);
                        }
                        (None, Some(dir)) => {
                            let flat = dir.join(format!("{name}.rs"));
                            let (file, child_dir) = if flat.is_file() {
                                (flat, dir.join(&name))
                            } else {
                                (dir.join(&name).join("mod.rs"), dir.join(&name))
                            };
                            self.read_file(&file, child_dir, child);
                        }
                        (None, None) => {}
                    }
                }
                Item::Impl(i) => self.read_impl(&path, i),
                Item::Macro(m) if m.mac.path.is_ident("stored_enum") => {
                    let parsed: StoredEnum = m.mac.parse_body().expect("stored_enum! parses");
                    self.def(&path, parsed.name, Def::Strings(parsed.values));
                }
                _ => {}
            }
        }
    }

    fn def(&mut self, path: &ModPath, name: String, def: Def) {
        self.modules
            .entry(path.clone())
            .or_default()
            .defs
            .insert(name, def);
    }

    fn read_use(&mut self, path: &ModPath, tree: &syn::UseTree, prefix: &mut Vec<String>) {
        match tree {
            syn::UseTree::Path(p) => {
                prefix.push(p.ident.to_string());
                self.read_use(path, &p.tree, prefix);
                prefix.pop();
            }
            syn::UseTree::Name(n) => {
                let name = n.ident.to_string();
                let mut full = prefix.clone();
                if name != "self" {
                    full.push(name.clone());
                }
                let alias = if name == "self" {
                    prefix.last().cloned().unwrap_or_default()
                } else {
                    name
                };
                self.modules
                    .entry(path.clone())
                    .or_default()
                    .uses
                    .insert(alias, full);
            }
            syn::UseTree::Rename(r) => {
                let mut full = prefix.clone();
                if r.ident != "self" {
                    full.push(r.ident.to_string());
                }
                self.modules
                    .entry(path.clone())
                    .or_default()
                    .uses
                    .insert(r.rename.to_string(), full);
            }
            syn::UseTree::Glob(_) => {
                self.modules
                    .entry(path.clone())
                    .or_default()
                    .globs
                    .push(prefix.clone());
            }
            syn::UseTree::Group(g) => {
                for tree in &g.items {
                    self.read_use(path, tree, prefix);
                }
            }
        }
    }

    fn read_impl(&mut self, path: &ModPath, item: &syn::ItemImpl) {
        let Some((_, trait_path, _)) = &item.trait_ else {
            return;
        };
        let Some(last) = trait_path.segments.last() else {
            return;
        };
        if last.ident != "Serialize" && last.ident != "Deserialize" {
            return;
        }
        let Type::Path(self_ty) = &*item.self_ty else {
            return;
        };
        let Some(name) = self_ty.path.segments.last() else {
            return;
        };
        let tokens = item.to_token_stream().to_string();
        self.modules
            .entry(path.clone())
            .or_default()
            .serde_impls
            .entry(name.ident.to_string())
            .or_default()
            .push(tokens);
    }

    /// Where a path written in `from` leads.
    fn resolve(&self, from: &ModPath, segments: &[String]) -> Target {
        self.resolve_depth(from, segments, 0)
    }

    fn resolve_depth(&self, from: &ModPath, segments: &[String], depth: usize) -> Target {
        let external = || Target::External(segments.last().cloned().unwrap_or_default());
        if depth > 16 || segments.is_empty() {
            return external();
        }
        let (first, rest) = segments.split_first().expect("not empty");
        if rest.is_empty() {
            return self.lookup(from, first, depth);
        }
        // The module the first segment names.
        let mut module = match first.as_str() {
            "crate" => Vec::new(),
            "self" => from.clone(),
            "super" => from[..from.len().saturating_sub(1)].to_vec(),
            name if self.has_child(from, name) => {
                let mut m = from.clone();
                m.push(name.to_string());
                m
            }
            name => match self.modules.get(from).and_then(|m| m.uses.get(name)) {
                Some(used) => match self.module_of(from, used, depth) {
                    Some(m) => m,
                    None => return external(),
                },
                None => return external(),
            },
        };
        let (last, middle) = rest.split_last().expect("not empty");
        for seg in middle {
            match seg.as_str() {
                "super" => {
                    module.pop();
                }
                name if self.has_child(&module, name) => module.push(name.to_string()),
                _ => return external(),
            }
        }
        self.lookup(&module, last, depth)
    }

    /// The module a `use` path names, if it names one.
    fn module_of(&self, from: &ModPath, segments: &[String], depth: usize) -> Option<ModPath> {
        let mut module = match segments.first()?.as_str() {
            "crate" => Vec::new(),
            "self" => from.clone(),
            "super" => from[..from.len().saturating_sub(1)].to_vec(),
            name if self.has_child(from, name) => {
                let mut m = from.clone();
                m.push(name.to_string());
                m
            }
            name => {
                let used = self.modules.get(from)?.uses.get(name)?;
                if depth > 16 {
                    return None;
                }
                self.module_of(from, &used.clone(), depth + 1)?
            }
        };
        for seg in &segments[1..] {
            match seg.as_str() {
                "super" => {
                    module.pop();
                }
                name if self.has_child(&module, name) => module.push(name.to_string()),
                _ => return None,
            }
        }
        Some(module)
    }

    fn has_child(&self, module: &ModPath, name: &str) -> bool {
        self.modules
            .get(module)
            .is_some_and(|m| m.children.contains(name))
    }

    /// A single name as seen from inside `module`: its own types, then its `use`s, then its glob imports.
    fn lookup(&self, module: &ModPath, name: &str, depth: usize) -> Target {
        let Some(m) = self.modules.get(module) else {
            return Target::External(name.to_string());
        };
        if m.defs.contains_key(name) {
            return Target::Local(module.clone(), name.to_string());
        }
        if let Some(used) = m.uses.get(name) {
            return self.resolve_depth(module, used, depth + 1);
        }
        for glob in &m.globs {
            if let Some(source) = self.module_of(module, glob, depth + 1)
                && source != *module
                && let Target::Local(p, n) = self.lookup(&source, name, depth + 1)
            {
                return Target::Local(p, n);
            }
        }
        Target::External(name.to_string())
    }
}

/// `stored_enum! { /// docs  pub enum Name ("what") { /// docs  Variant = "stored", ... } }`
struct StoredEnum {
    name: String,
    values: BTreeSet<String>,
}

impl syn::parse::Parse for StoredEnum {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        input.call(Attribute::parse_outer)?;
        input.parse::<syn::Visibility>()?;
        input.parse::<Token![enum]>()?;
        let name: syn::Ident = input.parse()?;
        let what;
        syn::parenthesized!(what in input);
        what.parse::<syn::Lit>()?;
        let body;
        syn::braced!(body in input);
        let mut values = BTreeSet::new();
        while !body.is_empty() {
            body.call(Attribute::parse_outer)?;
            body.parse::<syn::Ident>()?;
            body.parse::<Token![=]>()?;
            let stored: syn::Expr = body.parse()?;
            values.insert(match &stored {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) => s.value(),
                other => other.to_token_stream().to_string().replace(' ', ""),
            });
            if body.is_empty() {
                break;
            }
            body.parse::<Token![,]>()?;
        }
        Ok(StoredEnum {
            name: name.to_string(),
            values,
        })
    }
}

fn item_attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Struct(i) => &i.attrs,
        Item::Enum(i) => &i.attrs,
        Item::Type(i) => &i.attrs,
        Item::Use(i) => &i.attrs,
        Item::Mod(i) => &i.attrs,
        Item::Impl(i) => &i.attrs,
        Item::Macro(i) => &i.attrs,
        _ => &[],
    }
}

/// `#[cfg(test)]` (alone or inside `all(...)`).
fn is_test_only(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg") && {
            let tokens = a.meta.to_token_stream().to_string();
            tokens.contains("test") && !tokens.contains("not (test)") && !tokens.contains("any")
        }
    })
}

// ---- From definitions to shapes ----

/// The serde attributes of one item, field or variant.
#[derive(Default)]
struct Serde {
    rename: Option<String>,
    rename_all: Option<String>,
    default: bool,
    omittable: bool,
    skip: bool,
    flatten: bool,
    /// Kept as written: `tag=kind`, `untagged`, `with=...`, `alias=x`, `other`, ...
    rest: Vec<String>,
}

fn serde_of(attrs: &[Attribute]) -> Serde {
    let mut out = Serde::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
        attr.parse_nested_meta(|meta| {
            let key = meta.path.to_token_stream().to_string();
            let value = if meta.input.peek(Token![=]) {
                let lit: syn::LitStr = meta.value()?.parse()?;
                Some(lit.value())
            } else if meta.input.peek(syn::token::Paren) {
                let content;
                syn::parenthesized!(content in meta.input);
                let tokens: proc_macro2::TokenStream = content.parse()?;
                Some(tokens.to_string())
            } else {
                None
            };
            match key.as_str() {
                "rename" => out.rename = value,
                "rename_all" => out.rename_all = value,
                "default" => out.default = true,
                "skip_serializing_if" | "skip_serializing" => out.omittable = true,
                "skip" => out.skip = true,
                "flatten" => out.flatten = true,
                _ => out.rest.push(match value {
                    Some(v) => format!("{key}={v}"),
                    None => key,
                }),
            }
            Ok(())
        })
        .unwrap_or_else(|e| panic!("reading #[serde(...)]: {e}"));
    }
    out.rest.sort();
    out
}

/// serde's `rename_all` rules, for the names this crate uses (a Rust field is snake_case, a variant PascalCase).
fn rename(name: &str, rule: Option<&str>, is_variant: bool) -> String {
    let Some(rule) = rule else {
        return name.to_string();
    };
    let words: Vec<String> = if is_variant {
        let mut words = Vec::new();
        let mut word = String::new();
        for c in name.chars() {
            if c.is_uppercase() && !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            word.extend(c.to_lowercase());
        }
        words.push(word);
        words
    } else {
        name.split('_').map(str::to_string).collect()
    };
    let cap = |w: &String| {
        let mut c = w.chars();
        c.next()
            .map(|f| f.to_uppercase().chain(c).collect::<String>())
            .unwrap_or_default()
    };
    match rule {
        "snake_case" => words.join("_"),
        "kebab-case" => words.join("-"),
        "SCREAMING_SNAKE_CASE" => words.join("_").to_uppercase(),
        "SCREAMING-KEBAB-CASE" => words.join("-").to_uppercase(),
        "lowercase" => words.concat(),
        "UPPERCASE" => words.concat().to_uppercase(),
        "PascalCase" => words.iter().map(cap).collect(),
        "camelCase" => {
            let pascal: String = words.iter().map(cap).collect();
            let mut c = pascal.chars();
            c.next()
                .map(|f| f.to_lowercase().chain(c).collect())
                .unwrap_or_default()
        }
        other => panic!(
            "rename_all = {other:?} isn't known to the protocol snapshot; add it to `rename`"
        ),
    }
}

/// Types serde writes the same way as their content: `Box<T>` is `T`, `&str` is `String`.
fn normalize(name: &str, args: Vec<Ty>) -> Ty {
    match (name, args.len()) {
        ("Box" | "Arc" | "Rc", 1) => args.into_iter().next().expect("one argument"),
        ("str" | "Cow", _) if name == "str" || args.first().is_some_and(|a| a.name == "String") => {
            Ty::leaf("String")
        }
        ("Path", 0) => Ty::leaf("PathBuf"),
        ("HashMap", _) => Ty {
            name: "BTreeMap".into(),
            args,
        },
        ("HashSet", _) => Ty {
            name: "BTreeSet".into(),
            args,
        },
        _ => Ty {
            name: name.to_string(),
            args,
        },
    }
}

/// Walks the types reachable from the root and describes them.
pub fn extract(krate: &Crate, root_module: &str, root: &str, protocol_version: u32) -> Schema {
    let root_module: ModPath = root_module
        .split("::")
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    let root_target = krate.lookup(&root_module, root, 0);
    assert!(
        matches!(root_target, Target::Local(..)),
        "the protocol snapshot can't find {root} in {root_module:?}"
    );
    let mut walker = Walker {
        krate,
        shapes: BTreeMap::new(),
        queue: vec![root_target.clone()],
    };
    while let Some(target) = walker.queue.pop() {
        if walker.shapes.contains_key(&target) {
            continue;
        }
        let shape = walker.describe(&target);
        walker.shapes.insert(target, shape);
    }
    // Short names, or the full path where two types share one.
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for target in walker.shapes.keys() {
        if let Target::Local(_, name) = target {
            *counts.entry(name.as_str()).or_default() += 1;
        }
    }
    let names: HashMap<Target, String> = walker
        .shapes
        .keys()
        .map(|t| (t.clone(), display_name(t, &counts)))
        .collect();
    let types = walker
        .shapes
        .into_iter()
        .map(|(target, shape)| (names[&target].clone(), shape.finish(&names)))
        .collect();
    Schema {
        protocol_version,
        root: names[&root_target].clone(),
        types,
    }
}

fn display_name(target: &Target, counts: &HashMap<&str, usize>) -> String {
    match target {
        Target::Local(path, name) if counts[name.as_str()] > 1 => {
            let mut full = vec!["crate".to_string()];
            full.extend(path.iter().cloned());
            full.push(name.clone());
            full.join("::")
        }
        Target::Local(_, name) | Target::External(name) => name.clone(),
    }
}

/// A type expression whose local types aren't named yet.
#[derive(Clone)]
enum RawTy {
    Named(Target, Vec<RawTy>),
    Leaf(Ty),
    Tuple(Vec<RawTy>),
    Array(Box<RawTy>, String),
}

impl RawTy {
    fn finish(&self, names: &HashMap<Target, String>) -> Ty {
        match self {
            RawTy::Named(target, args) => normalize(
                names
                    .get(target)
                    .map(String::as_str)
                    .unwrap_or_else(|| match target {
                        Target::Local(_, n) | Target::External(n) => n,
                    }),
                args.iter().map(|a| a.finish(names)).collect(),
            ),
            RawTy::Leaf(ty) => ty.clone(),
            RawTy::Tuple(items) => Ty {
                name: "()".into(),
                args: items.iter().map(|a| a.finish(names)).collect(),
            },
            RawTy::Array(item, len) => Ty {
                name: format!("[;{len}]"),
                args: vec![item.finish(names)],
            },
        }
    }
}

enum RawShape {
    Struct(Vec<String>, BTreeMap<String, RawField>),
    Tuple(Vec<String>, Vec<RawTy>),
    Unit,
    Enum(Vec<String>, BTreeMap<String, RawVariant>),
    Alias(RawTy),
    Strings(BTreeSet<String>),
    Custom(String),
}

struct RawField {
    ty: RawTy,
    required: bool,
    omittable: bool,
    attrs: Vec<String>,
}

struct RawVariant {
    attrs: Vec<String>,
    kind: RawVariantKind,
}

enum RawVariantKind {
    Unit,
    Newtype(RawTy),
    Tuple(Vec<RawTy>),
    Struct(BTreeMap<String, RawField>),
}

impl RawShape {
    fn finish(self, names: &HashMap<Target, String>) -> Shape {
        let fields = |fields: BTreeMap<String, RawField>| {
            fields
                .into_iter()
                .map(|(k, f)| {
                    (
                        k,
                        Field {
                            ty: f.ty.finish(names).to_string(),
                            required: f.required,
                            omittable: f.omittable,
                            attrs: f.attrs,
                        },
                    )
                })
                .collect()
        };
        match self {
            RawShape::Struct(attrs, f) => Shape::Struct {
                attrs,
                fields: fields(f),
            },
            RawShape::Tuple(attrs, items) => Shape::Tuple {
                attrs,
                items: items.iter().map(|t| t.finish(names).to_string()).collect(),
            },
            RawShape::Unit => Shape::Unit,
            RawShape::Enum(attrs, variants) => Shape::Enum {
                attrs,
                variants: variants
                    .into_iter()
                    .map(|(k, v)| {
                        let mut variant = Variant {
                            attrs: v.attrs,
                            newtype: None,
                            tuple: None,
                            fields: None,
                        };
                        match v.kind {
                            RawVariantKind::Unit => {}
                            RawVariantKind::Newtype(t) => {
                                variant.newtype = Some(t.finish(names).to_string())
                            }
                            RawVariantKind::Tuple(items) => {
                                variant.tuple = Some(
                                    items.iter().map(|t| t.finish(names).to_string()).collect(),
                                )
                            }
                            RawVariantKind::Struct(f) => variant.fields = Some(fields(f)),
                        }
                        (k, variant)
                    })
                    .collect(),
            },
            RawShape::Alias(ty) => Shape::Alias {
                ty: ty.finish(names).to_string(),
            },
            RawShape::Strings(values) => Shape::Strings { values },
            RawShape::Custom(serde_impl) => Shape::Custom { serde_impl },
        }
    }
}

struct Walker<'a> {
    krate: &'a Crate,
    shapes: BTreeMap<Target, RawShape>,
    queue: Vec<Target>,
}

impl Walker<'_> {
    fn describe(&mut self, target: &Target) -> RawShape {
        let Target::Local(module, name) = target else {
            unreachable!("only local types are queued")
        };
        let m = &self.krate.modules[module];
        if let Some(impls) = m.serde_impls.get(name) {
            let mut hash = Sha256::new();
            for tokens in impls {
                hash.update(tokens.as_bytes());
            }
            let hex: String = hash
                .finalize()
                .iter()
                .take(8)
                .map(|b| format!("{b:02x}"))
                .collect();
            return RawShape::Custom(hex);
        }
        match m.defs[name].clone() {
            Def::Strings(values) => RawShape::Strings(values),
            Def::Alias(alias) => {
                let generics = generic_names(&alias.generics);
                RawShape::Alias(self.ty(module, &alias.ty, &generics))
            }
            Def::Struct(s) => {
                let generics = generic_names(&s.generics);
                let serde = serde_of(&s.attrs);
                let attrs = container_attrs(&serde);
                match &s.fields {
                    Fields::Named(_) => {
                        RawShape::Struct(attrs, self.fields(module, &s.fields, &serde, &generics))
                    }
                    Fields::Unnamed(f) => RawShape::Tuple(
                        attrs,
                        f.unnamed
                            .iter()
                            .filter(|f| !serde_of(&f.attrs).skip)
                            .map(|f| self.ty(module, &f.ty, &generics))
                            .collect(),
                    ),
                    Fields::Unit => RawShape::Unit,
                }
            }
            Def::Enum(e) => {
                let generics = generic_names(&e.generics);
                let serde = serde_of(&e.attrs);
                let mut variants = BTreeMap::new();
                for v in &e.variants {
                    let vs = serde_of(&v.attrs);
                    if vs.skip {
                        continue;
                    }
                    let wire = vs.rename.clone().unwrap_or_else(|| {
                        rename(&v.ident.to_string(), serde.rename_all.as_deref(), true)
                    });
                    let kind = match &v.fields {
                        Fields::Unit => RawVariantKind::Unit,
                        Fields::Unnamed(f) if f.unnamed.len() == 1 => {
                            RawVariantKind::Newtype(self.ty(module, &f.unnamed[0].ty, &generics))
                        }
                        Fields::Unnamed(f) => RawVariantKind::Tuple(
                            f.unnamed
                                .iter()
                                .map(|f| self.ty(module, &f.ty, &generics))
                                .collect(),
                        ),
                        Fields::Named(_) => {
                            RawVariantKind::Struct(self.fields(module, &v.fields, &vs, &generics))
                        }
                    };
                    let mut attrs = vs.rest.clone();
                    if vs.default {
                        attrs.push("default".into());
                    }
                    attrs.sort();
                    variants.insert(wire, RawVariant { attrs, kind });
                }
                RawShape::Enum(container_attrs(&serde), variants)
            }
        }
    }

    fn fields(
        &mut self,
        module: &ModPath,
        fields: &Fields,
        container: &Serde,
        generics: &HashSet<String>,
    ) -> BTreeMap<String, RawField> {
        let mut out = BTreeMap::new();
        for (i, field) in fields.iter().enumerate() {
            let fs = serde_of(&field.attrs);
            if fs.skip {
                continue;
            }
            let ident = field
                .ident
                .as_ref()
                .map(|i| i.to_string())
                .unwrap_or_else(|| i.to_string());
            let ident = ident.trim_start_matches("r#").to_string();
            let ty = self.ty(module, &field.ty, generics);
            let is_option = matches!(&field.ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "Option"));
            let mut attrs = fs.rest.clone();
            let key = if fs.flatten {
                attrs.push("flatten".into());
                attrs.sort();
                format!("..{ident}")
            } else {
                fs.rename
                    .clone()
                    .unwrap_or_else(|| rename(&ident, container.rename_all.as_deref(), false))
            };
            let custom_read = attrs
                .iter()
                .any(|a| a.starts_with("with=") || a.starts_with("deserialize_with="));
            let required = !fs.flatten
                && !fs.default
                && !container.default
                && !(is_option && !custom_read)
                && !attrs.iter().any(|a| a == "skip_deserializing");
            out.insert(
                key,
                RawField {
                    ty,
                    required,
                    omittable: fs.omittable,
                    attrs,
                },
            );
        }
        out
    }

    fn ty(&mut self, module: &ModPath, ty: &Type, generics: &HashSet<String>) -> RawTy {
        match ty {
            Type::Reference(r) => self.ty(module, &r.elem, generics),
            Type::Paren(p) => self.ty(module, &p.elem, generics),
            Type::Group(g) => self.ty(module, &g.elem, generics),
            Type::Tuple(t) => RawTy::Tuple(
                t.elems
                    .iter()
                    .map(|e| self.ty(module, e, generics))
                    .collect(),
            ),
            Type::Slice(s) => RawTy::Named(
                Target::External("Vec".into()),
                vec![self.ty(module, &s.elem, generics)],
            ),
            Type::Array(a) => RawTy::Array(
                Box::new(self.ty(module, &a.elem, generics)),
                a.len.to_token_stream().to_string(),
            ),
            Type::Path(p) if p.qself.is_none() => {
                let segments: Vec<String> = p
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect();
                let last = p.path.segments.last().expect("a path has a segment");
                let args: Vec<RawTy> = match &last.arguments {
                    PathArguments::AngleBracketed(a) => a
                        .args
                        .iter()
                        .filter_map(|arg| match arg {
                            GenericArgument::Type(t) => Some(self.ty(module, t, generics)),
                            _ => None,
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                if segments.len() == 1 && generics.contains(&segments[0]) {
                    return RawTy::Leaf(Ty::leaf(segments[0].clone()));
                }
                let target = self.krate.resolve(module, &segments);
                if matches!(target, Target::Local(..)) && !self.shapes.contains_key(&target) {
                    self.queue.push(target.clone());
                }
                RawTy::Named(target, args)
            }
            other => RawTy::Leaf(Ty::leaf(
                other.to_token_stream().to_string().replace(' ', ""),
            )),
        }
    }
}

fn generic_names(generics: &syn::Generics) -> HashSet<String> {
    generics
        .type_params()
        .map(|p| p.ident.to_string())
        .collect()
}

/// The container attributes that change the wire (renames are already applied to the names).
fn container_attrs(serde: &Serde) -> Vec<String> {
    let mut attrs = serde.rest.clone();
    if serde.default {
        attrs.push("default".into());
    }
    attrs.sort();
    attrs
}
