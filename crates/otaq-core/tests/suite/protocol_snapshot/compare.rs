//! Compares the committed schema with the current one, walking both from the root by wire name, so a Rust-only
//! rename (a type, a field with `#[serde(rename)]`) isn't a change. Each difference is either compatible (an older
//! service and a newer app still understand each other) or not.

use std::collections::{BTreeMap, HashSet};

use super::schema::{Field, Schema, Shape, Ty, Variant};

#[derive(Debug, Default, PartialEq)]
pub struct Changes {
    /// Fine without a new protocol number: "Ticket: new optional field `pinned`".
    pub compatible: Vec<String>,
    /// Need `PROTOCOL_VERSION` bumped: "MoveParams: field `to` removed".
    pub incompatible: Vec<String>,
}

pub fn compare(old: &Schema, new: &Schema) -> Changes {
    let mut walk = Walk {
        old,
        new,
        seen: HashSet::new(),
        changes: Changes::default(),
    };
    walk.ty(&Ty::parse(&old.root), &Ty::parse(&new.root), &new.root);
    walk.changes.compatible.sort();
    walk.changes.incompatible.sort();
    walk.changes
}

struct Walk<'a> {
    old: &'a Schema,
    new: &'a Schema,
    seen: HashSet<(String, String)>,
    changes: Changes,
}

impl Walk<'_> {
    fn bad(&mut self, what: String) {
        self.changes.incompatible.push(what);
    }

    fn fine(&mut self, what: String) {
        self.changes.compatible.push(what);
    }

    fn ty(&mut self, old: &Ty, new: &Ty, at: &str) {
        let old_shape = self.old.types.get(&old.name);
        let new_shape = self.new.types.get(&new.name);
        match (old_shape, new_shape) {
            (Some(o), Some(n)) => {
                if self.seen.insert((old.name.clone(), new.name.clone())) {
                    self.shape(o, n, &new.name);
                }
            }
            (None, None) if old.name == new.name => {}
            _ => {
                return self.bad(format!("{at}: type changed from `{old}` to `{new}`"));
            }
        }
        if old.args.len() != new.args.len() {
            return self.bad(format!("{at}: type changed from `{old}` to `{new}`"));
        }
        for (o, n) in old.args.iter().zip(&new.args) {
            self.ty(o, n, at);
        }
    }

    fn ty_text(&mut self, old: &str, new: &str, at: &str) {
        self.ty(&Ty::parse(old), &Ty::parse(new), at);
    }

    fn shape(&mut self, old: &Shape, new: &Shape, name: &str) {
        match (old, new) {
            (
                Shape::Struct {
                    attrs: oa,
                    fields: of,
                },
                Shape::Struct {
                    attrs: na,
                    fields: nf,
                },
            ) => {
                if oa != na {
                    self.bad(format!(
                        "{name}: serde attributes changed from {oa:?} to {na:?}"
                    ));
                }
                self.fields(of, nf, name, na);
            }
            (
                Shape::Tuple {
                    attrs: oa,
                    items: oi,
                },
                Shape::Tuple {
                    attrs: na,
                    items: ni,
                },
            ) => {
                if oa != na || oi.len() != ni.len() {
                    self.bad(format!("{name}: changed from {old:?} to {new:?}"));
                    return;
                }
                for (o, n) in oi.iter().zip(ni) {
                    self.ty_text(o, n, name);
                }
            }
            (Shape::Unit, Shape::Unit) => {}
            (
                Shape::Enum {
                    attrs: oa,
                    variants: ov,
                },
                Shape::Enum {
                    attrs: na,
                    variants: nv,
                },
            ) => {
                if oa != na {
                    self.bad(format!(
                        "{name}: serde attributes changed from {oa:?} to {na:?}"
                    ));
                }
                let untagged = na.iter().any(|a| a == "untagged");
                for (wire, o) in ov {
                    match nv.get(wire) {
                        Some(n) => self.variant(o, n, &format!("{name} `{wire}`")),
                        None => self.bad(format!("{name}: variant `{wire}` removed or renamed")),
                    }
                }
                for wire in nv.keys().filter(|w| !ov.contains_key(*w)) {
                    if untagged {
                        self.bad(format!("{name}: new variant `{wire}` in an untagged enum"));
                    } else {
                        self.fine(format!("{name}: new variant `{wire}`"));
                    }
                }
            }
            (Shape::Alias { ty: o }, Shape::Alias { ty: n }) => self.ty_text(o, n, name),
            (Shape::Strings { values: o }, Shape::Strings { values: n }) => {
                for gone in o.difference(n) {
                    self.bad(format!("{name}: value `{gone}` removed or renamed"));
                }
                for added in n.difference(o) {
                    self.fine(format!("{name}: new value `{added}`"));
                }
            }
            (Shape::Custom { serde_impl: o }, Shape::Custom { serde_impl: n }) => {
                if o != n {
                    self.bad(format!(
                        "{name}: its hand-written Serialize/Deserialize changed"
                    ));
                }
            }
            _ => self.bad(format!(
                "{name}: changed from {} to {}",
                kind(old),
                kind(new)
            )),
        }
    }

    fn fields(
        &mut self,
        old: &BTreeMap<String, Field>,
        new: &BTreeMap<String, Field>,
        name: &str,
        new_attrs: &[String],
    ) {
        for (wire, o) in old {
            let at = format!("{name}.{wire}");
            let Some(n) = new.get(wire) else {
                self.bad(format!("{name}: field `{wire}` removed or renamed"));
                continue;
            };
            if o.required != n.required {
                let now = if n.required { "required" } else { "optional" };
                self.bad(format!("{at}: now {now}"));
            } else if o.omittable != n.omittable && n.required {
                self.bad(format!("{at}: a required field that may now be left out"));
            }
            if o.attrs != n.attrs {
                self.bad(format!(
                    "{at}: serde attributes changed from {:?} to {:?}",
                    o.attrs, n.attrs
                ));
            }
            self.ty_text(&o.ty, &n.ty, &at);
        }
        let closed = new_attrs.iter().any(|a| a == "deny_unknown_fields");
        for (wire, n) in new.iter().filter(|(w, _)| !old.contains_key(*w)) {
            if n.required {
                self.bad(format!("{name}: new required field `{wire}`"));
            } else if closed {
                self.bad(format!(
                    "{name}: new field `{wire}` where unknown fields are refused"
                ));
            } else if n.attrs.iter().any(|a| a == "flatten") {
                self.bad(format!("{name}: new flattened field `{wire}`"));
            } else {
                self.fine(format!("{name}: new optional field `{wire}`"));
            }
        }
    }

    fn variant(&mut self, old: &Variant, new: &Variant, at: &str) {
        if old.attrs != new.attrs {
            self.bad(format!(
                "{at}: serde attributes changed from {:?} to {:?}",
                old.attrs, new.attrs
            ));
        }
        match (old, new) {
            (
                Variant {
                    newtype: Some(o), ..
                },
                Variant {
                    newtype: Some(n), ..
                },
            ) => self.ty_text(o, n, at),
            (Variant { tuple: Some(o), .. }, Variant { tuple: Some(n), .. })
                if o.len() == n.len() =>
            {
                for (o, n) in o.iter().zip(n) {
                    self.ty_text(o, n, at);
                }
            }
            (
                Variant {
                    fields: Some(o), ..
                },
                Variant {
                    fields: Some(n), ..
                },
            ) => self.fields(o, n, at, &new.attrs),
            (
                Variant {
                    newtype: None,
                    tuple: None,
                    fields: None,
                    ..
                },
                Variant {
                    newtype: None,
                    tuple: None,
                    fields: None,
                    ..
                },
            ) => {}
            _ => self.bad(format!("{at}: its content changed from {old:?} to {new:?}")),
        }
    }
}

fn kind(shape: &Shape) -> &'static str {
    match shape {
        Shape::Struct { .. } => "a struct",
        Shape::Tuple { .. } => "a tuple struct",
        Shape::Unit => "a unit struct",
        Shape::Enum { .. } => "an enum",
        Shape::Alias { .. } => "a type alias",
        Shape::Strings { .. } => "a string enum",
        Shape::Custom { .. } => "hand-written serde",
    }
}
