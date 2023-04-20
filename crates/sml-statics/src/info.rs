//! See [`Info`].

use crate::{basis::Bs, env::Env};
use fast_hash::FxHashSet;
use sml_hir::la_arena;
use sml_statics_types::info::{IdStatus, ValInfo};
use sml_statics_types::ty::{Ty, TyData, TyScheme, Tys};
use sml_statics_types::util::ty_syms;
use sml_statics_types::{def, display::MetaVarNames, mode::Mode, sym::Syms};
use std::fmt;

pub(crate) type IdxMap<K, V> = la_arena::ArenaMap<la_arena::Idx<K>, V>;

#[derive(Debug, Default, Clone)]
pub(crate) struct Defs {
  pub(crate) str_dec: IdxMap<sml_hir::StrDec, def::Def>,
  pub(crate) str_exp: IdxMap<sml_hir::StrExp, def::Def>,
  pub(crate) sig_exp: IdxMap<sml_hir::SigExp, def::Def>,
  pub(crate) spec: IdxMap<sml_hir::Spec, def::Def>,
  pub(crate) dec: IdxMap<sml_hir::Dec, def::Def>,
  pub(crate) exp: IdxMap<sml_hir::Exp, FxHashSet<def::Def>>,
  pub(crate) pat: IdxMap<sml_hir::Pat, FxHashSet<def::Def>>,
  pub(crate) ty: IdxMap<sml_hir::Ty, def::Def>,
}

impl Defs {
  fn get(&self, idx: sml_hir::Idx) -> FxHashSet<def::Def> {
    match idx {
      sml_hir::Idx::StrDec(idx) => self.str_dec.get(idx).into_iter().copied().collect(),
      sml_hir::Idx::StrExp(idx) => self.str_exp.get(idx).into_iter().copied().collect(),
      sml_hir::Idx::SigExp(idx) => self.sig_exp.get(idx).into_iter().copied().collect(),
      sml_hir::Idx::Spec(idx) => self.spec.get(idx).into_iter().copied().collect(),
      sml_hir::Idx::Dec(idx) => self.dec.get(idx).into_iter().copied().collect(),
      sml_hir::Idx::Exp(idx) => self.exp.get(idx).into_iter().flatten().copied().collect(),
      sml_hir::Idx::Pat(idx) => self.pat.get(idx).into_iter().flatten().copied().collect(),
      sml_hir::Idx::Ty(idx) => self.ty.get(idx).into_iter().copied().collect(),
    }
  }

  fn with_def(&self, def: def::Def) -> impl Iterator<Item = sml_hir::Idx> + '_ {
    std::iter::empty::<(sml_hir::Idx, def::Def)>()
      .chain(self.str_dec.iter().map(|(idx, &d)| (idx.into(), d)))
      .chain(self.str_exp.iter().map(|(idx, &d)| (idx.into(), d)))
      .chain(self.sig_exp.iter().map(|(idx, &d)| (idx.into(), d)))
      .chain(self.spec.iter().map(|(idx, &d)| (idx.into(), d)))
      .chain(self.dec.iter().map(|(idx, &d)| (idx.into(), d)))
      .chain(self.ty.iter().map(|(idx, &d)| (idx.into(), d)))
      .chain(self.exp.iter().flat_map(|(idx, ds)| ds.iter().map(move |&d| (idx.into(), d))))
      .chain(self.pat.iter().flat_map(|(idx, ds)| ds.iter().map(move |&d| (idx.into(), d))))
      .filter_map(move |(idx, d)| (d == def).then_some(idx))
  }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct Docs {
  pub(crate) str_dec: IdxMap<sml_hir::StrDec, String>,
  pub(crate) spec: IdxMap<sml_hir::Spec, String>,
  pub(crate) dec: IdxMap<sml_hir::Dec, String>,
  pub(crate) pat: IdxMap<sml_hir::Pat, String>,
}

impl Docs {
  fn get(&self, idx: sml_hir::Idx) -> Option<&str> {
    let ret = match idx {
      sml_hir::Idx::StrDec(idx) => self.str_dec.get(idx)?,
      sml_hir::Idx::Spec(idx) => self.spec.get(idx)?,
      sml_hir::Idx::Dec(idx) => self.dec.get(idx)?,
      sml_hir::Idx::Pat(idx) => self.pat.get(idx)?,
      _ => return None,
    };
    Some(ret.as_str())
  }

  fn try_insert(&mut self, idx: sml_hir::Idx, doc: String) {
    match idx {
      sml_hir::Idx::StrDec(idx) => {
        self.str_dec.insert(idx, doc);
      }
      sml_hir::Idx::Spec(idx) => {
        self.spec.insert(idx, doc);
      }
      sml_hir::Idx::Dec(idx) => {
        self.dec.insert(idx, doc);
      }
      sml_hir::Idx::Pat(idx) => {
        self.pat.insert(idx, doc);
      }
      _ => {}
    }
  }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct TyEntries {
  pub(crate) exp: IdxMap<sml_hir::Exp, TyEntry>,
  pub(crate) pat: IdxMap<sml_hir::Pat, TyEntry>,
  pub(crate) ty: IdxMap<sml_hir::Ty, TyEntry>,
}

impl TyEntries {
  fn get(&self, idx: sml_hir::Idx) -> Option<&TyEntry> {
    match idx {
      sml_hir::Idx::Exp(idx) => self.exp.get(idx),
      sml_hir::Idx::Pat(idx) => self.pat.get(idx),
      sml_hir::Idx::Ty(idx) => self.ty.get(idx),
      _ => None,
    }
  }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct Entries {
  pub(crate) defs: Defs,
  pub(crate) docs: Docs,
  pub(crate) tys: TyEntries,
}

/// Information about HIR indices.
#[derive(Debug, Clone)]
pub struct Info {
  pub(crate) mode: Mode,
  pub(crate) entries: Entries,
  pub(crate) bs: Bs,
}

impl Info {
  pub(crate) fn new(mode: Mode) -> Self {
    Self { mode, entries: Entries::default(), bs: Bs::default() }
  }

  /// Adds documentation for the index.
  pub fn add_doc(&mut self, idx: sml_hir::Idx, doc: String) {
    self.entries.docs.try_insert(idx, doc);
  }

  /// Returns a Markdown string with type information associated with this index.
  #[must_use]
  pub fn get_ty_md(&self, syms: &Syms, tys: &Tys, idx: sml_hir::Idx) -> Option<String> {
    let ty_entry = self.entries.tys.get(idx)?;
    let ty_entry = TyEntryDisplay { ty_entry, syms, tys };
    Some(ty_entry.to_string())
  }

  /// Returns documentation for this index.
  #[must_use]
  pub fn get_doc(&self, idx: sml_hir::Idx) -> Option<&str> {
    self.entries.docs.get(idx)
  }

  /// Returns the definition sites of the idx.
  #[must_use]
  pub fn get_defs(&self, idx: sml_hir::Idx) -> FxHashSet<def::Def> {
    self.entries.defs.get(idx)
  }

  /// Returns the definition site of the type for the idx.
  #[must_use]
  pub fn get_ty_defs(&self, syms: &Syms, tys: &Tys, idx: sml_hir::Idx) -> Option<Vec<def::Def>> {
    let ty_entry = self.entries.tys.get(idx)?;
    let mut ret = Vec::<def::Def>::new();
    ty_syms(tys, ty_entry.ty, &mut |sym| match syms.get(sym) {
      None => {}
      Some(sym_info) => match sym_info.ty_info.def {
        None => {}
        Some(def) => ret.push(def),
      },
    });
    Some(ret)
  }

  /// Gets the variants for the type of the index. The bool is whether the name has an argument.
  #[must_use]
  pub fn get_variants(
    &self,
    syms: &Syms,
    tys: &Tys,
    idx: sml_hir::Idx,
  ) -> Option<Vec<(str_util::Name, bool)>> {
    let ty_entry = self.entries.tys.get(idx)?;
    let sym = match tys.data(ty_entry.ty) {
      TyData::Con(data) => data.sym,
      _ => return None,
    };
    let mut ret: Vec<_> = syms
      .get(sym)?
      .ty_info
      .val_env
      .iter()
      .map(|(name, val_info)| {
        let has_arg = matches!(tys.data(val_info.ty_scheme.ty), TyData::Fn(_));
        (name.clone(), has_arg)
      })
      .collect();
    ret.sort_unstable();
    Some(ret)
  }

  /// Returns the symbols for this file.
  ///
  /// You also have to pass down the `path` that this `Info` is for. It's slightly odd, but we
  /// need it to know which `Def`s we should actually include in the return value.
  #[must_use]
  pub fn document_symbols(
    &self,
    syms: &Syms,
    tys: &Tys,
    path: paths::PathId,
  ) -> Vec<DocumentSymbol> {
    let mut mvs = MetaVarNames::new(tys);
    let mut ret = Vec::<DocumentSymbol>::new();
    ret.extend(self.bs.fun_env.iter().filter_map(|(name, fun_sig)| {
      let idx = def_idx(path, fun_sig.body_env.def?)?;
      let mut children = Vec::<DocumentSymbol>::new();
      env_syms(&mut children, &mut mvs, syms, tys, path, &fun_sig.body_env);
      Some(DocumentSymbol {
        name: name.as_str().to_owned(),
        kind: sml_namespace::SymbolKind::Functor,
        detail: None,
        idx,
        children,
      })
    }));
    ret.extend(self.bs.sig_env.iter().filter_map(|(name, sig)| {
      let idx = def_idx(path, sig.env.def?)?;
      let mut children = Vec::<DocumentSymbol>::new();
      env_syms(&mut children, &mut mvs, syms, tys, path, &sig.env);
      Some(DocumentSymbol {
        name: name.as_str().to_owned(),
        kind: sml_namespace::SymbolKind::Signature,
        detail: None,
        idx,
        children,
      })
    }));
    env_syms(&mut ret, &mut mvs, syms, tys, path, &self.bs.env);
    // order doesn't seem to matter. at least vs code displays the symbols in source order.
    ret
  }

  /// Returns indices that have the given definition.
  pub fn get_with_def(&self, def: def::Def) -> impl Iterator<Item = sml_hir::Idx> + '_ {
    self.entries.defs.with_def(def)
  }

  /// Returns the completions for this file.
  #[must_use]
  pub fn completions(&self, syms: &Syms, tys: &Tys) -> Vec<CompletionItem> {
    let mut ret = Vec::<CompletionItem>::new();
    let mut mvs = MetaVarNames::new(tys);
    ret.extend(self.bs.env.val_env.iter().map(|(name, val_info)| {
      mvs.clear();
      mvs.extend_for(val_info.ty_scheme.ty);
      CompletionItem {
        label: name.as_str().to_owned(),
        kind: val_info_symbol_kind(tys, val_info),
        detail: Some(val_info.ty_scheme.display(&mvs, syms).to_string()),
        // TODO improve? might need to reorganize where documentation is stored
        documentation: None,
      }
    }));
    ret
  }

  /// Returns some type annotation bits.
  pub fn show_ty_annot<'a>(
    &'a self,
    syms: &'a Syms,
    tys: &'a Tys,
  ) -> impl Iterator<Item = (sml_hir::la_arena::Idx<sml_hir::Pat>, String)> + 'a {
    self.entries.tys.pat.iter().filter_map(|(pat, ty_entry)| {
      let self_def = self.entries.defs.pat.get(pat)?.iter().any(|&d| match d {
        def::Def::Path(_, ref_idx) => match ref_idx {
          sml_hir::Idx::Pat(ref_pat) => pat == ref_pat,
          _ => false,
        },
        def::Def::Primitive(_) => false,
      });
      if !self_def {
        return None;
      }
      let mut mvs = MetaVarNames::new(tys);
      mvs.extend_for(ty_entry.ty);
      let ty = ty_entry.ty.display(&mvs, syms);
      Some((pat, format!(" : {ty})")))
    })
  }
}

#[derive(Debug, Clone)]
pub(crate) struct TyEntry {
  ty: Ty,
  /// invariant: if this is `Some`, the ty scheme has non-empty bound ty vars.
  ty_scheme: Option<TyScheme>,
}

impl TyEntry {
  pub(crate) fn new(ty: Ty, ty_scheme: Option<TyScheme>) -> Self {
    Self { ty, ty_scheme: ty_scheme.and_then(|ts| (!ts.bound_vars.is_empty()).then_some(ts)) }
  }
}

struct TyEntryDisplay<'a> {
  ty_entry: &'a TyEntry,
  syms: &'a Syms,
  tys: &'a Tys,
}

impl fmt::Display for TyEntryDisplay<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let mut mvs = MetaVarNames::new(self.tys);
    mvs.extend_for(self.ty_entry.ty);
    writeln!(f, "```sml")?;
    if let Some(ty_scheme) = &self.ty_entry.ty_scheme {
      mvs.extend_for(ty_scheme.ty);
      let ty_scheme = ty_scheme.display(&mvs, self.syms);
      writeln!(f, "(* most general *)")?;
      writeln!(f, "{ty_scheme}")?;
      writeln!(f, "(* this usage *)")?;
    }
    let ty = self.ty_entry.ty.display(&mvs, self.syms);
    writeln!(f, "{ty}")?;
    writeln!(f, "```")?;
    Ok(())
  }
}

/// need to do extend instead of a big chain of chains because of the borrow checker.
fn env_syms(
  ac: &mut Vec<DocumentSymbol>,
  mvs: &mut MetaVarNames<'_>,
  syms: &Syms,
  tys: &Tys,
  path: paths::PathId,
  env: &Env,
) {
  ac.extend(env.str_env.iter().filter_map(|(name, env)| {
    let idx = def_idx(path, env.def?)?;
    let mut children = Vec::<DocumentSymbol>::new();
    env_syms(&mut children, mvs, syms, tys, path, env);
    Some(DocumentSymbol {
      name: name.as_str().to_owned(),
      kind: sml_namespace::SymbolKind::Structure,
      detail: None,
      idx,
      children,
    })
  }));
  ac.extend(env.ty_env.iter().filter_map(|(name, ty_info)| {
    mvs.clear();
    mvs.extend_for(ty_info.ty_scheme.ty);
    let idx = def_idx(path, ty_info.def?)?;
    Some(DocumentSymbol {
      name: name.as_str().to_owned(),
      kind: sml_namespace::SymbolKind::Type,
      detail: Some(ty_info.ty_scheme.display(mvs, syms).to_string()),
      idx,
      children: Vec::new(),
    })
  }));
  ac.extend(env.val_env.iter().flat_map(|(name, val_info)| {
    mvs.clear();
    mvs.extend_for(val_info.ty_scheme.ty);
    let detail = val_info.ty_scheme.display(mvs, syms).to_string();
    val_info.defs.iter().filter_map(move |&def| {
      let idx = def_idx(path, def)?;
      Some(DocumentSymbol {
        name: name.as_str().to_owned(),
        kind: val_info_symbol_kind(tys, val_info),
        detail: Some(detail.clone()),
        idx,
        children: Vec::new(),
      })
    })
  }));
}

fn def_idx(path: paths::PathId, def: def::Def) -> Option<sml_hir::Idx> {
  match def {
    def::Def::Path(p, idx) => match p {
      def::Path::Regular(p) => (p == path).then_some(idx),
      def::Path::BuiltinLib(_) => None,
    },
    def::Def::Primitive(_) => None,
  }
}

fn val_info_symbol_kind(tys: &Tys, val_info: &ValInfo) -> sml_namespace::SymbolKind {
  match val_info.id_status {
    IdStatus::Con => sml_namespace::SymbolKind::Constructor,
    IdStatus::Exn(_) => sml_namespace::SymbolKind::Exception,
    IdStatus::Val => match tys.data(val_info.ty_scheme.ty) {
      TyData::Fn(_) => sml_namespace::SymbolKind::Function,
      _ => sml_namespace::SymbolKind::Value,
    },
  }
}

/// A document symbol.
#[derive(Debug)]
pub struct DocumentSymbol {
  /// The name of the symbol.
  pub name: String,
  /// What kind of symbol this is.
  pub kind: sml_namespace::SymbolKind,
  /// Detail about this symbol.
  pub detail: Option<String>,
  /// The index of the symbol.
  pub idx: sml_hir::Idx,
  /// Children of this symbol.
  pub children: Vec<DocumentSymbol>,
}

/// A completion item.
#[derive(Debug)]
pub struct CompletionItem {
  /// The label.
  pub label: String,
  /// The kind.
  pub kind: sml_namespace::SymbolKind,
  /// Detail about it.
  pub detail: Option<String>,
  /// Markdown documentation for it.
  pub documentation: Option<String>,
}
