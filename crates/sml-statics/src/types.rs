//! Types.
//!
//! Probably the single most important file in this crate. Lots of types used pervasively across
//! this crate are defined here.

mod bound_var;
mod fixed_var;
mod meta_var;
mod overload;

use crate::def;
use drop_bomb::DropBomb;
use fast_hash::FxHashMap;
use std::{collections::BTreeMap, fmt};

pub(crate) use bound_var::BoundTyVar;
pub(crate) use fixed_var::{FixedTyVar, FixedTyVarGen, TyVarSrc};
pub(crate) use meta_var::{Generalizable, MetaTyVar, MetaTyVarGen, MetaTyVarGeneralizer};
pub(crate) use overload::{BasicOverload, CompositeOverload, Overload};

/// Definition: Type
#[derive(Debug, Clone)]
pub(crate) enum Ty {
  None,
  /// Can only appear when this Ty is wrapped in a TyScheme.
  BoundVar(BoundTyVar),
  MetaVar(MetaTyVar),
  FixedVar(FixedTyVar),
  /// Definition: RowType
  Record(RecordTy),
  /// Definition: ConsType
  ///
  /// Use `Ty::zero` if constructing a zero-argument `Con`.
  Con(Vec<Ty>, Sym),
  /// Definition: FunType
  ///
  /// Use `Ty::fun` if constructing a `Fn` from unboxed types.
  Fn(Box<Ty>, Box<Ty>),
}

impl Ty {
  /// Returns a [`Self::Con`] with 0 arguments and the given `sym`.
  pub(crate) const fn zero(sym: Sym) -> Self {
    Self::Con(Vec::new(), sym)
  }

  /// Returns a [`Self::Fn`] from `param` to `res`.
  pub(crate) fn fun(param: Self, res: Self) -> Self {
    Self::Fn(param.into(), res.into())
  }

  pub(crate) fn desc(&self) -> &'static str {
    match self {
      Ty::None => "an unknown type",
      Ty::BoundVar(_) => "a bound type variable",
      Ty::MetaVar(_) => "an unsolved type variable",
      Ty::FixedVar(_) => "a fixed type variable",
      Ty::Record(_) => "a record or tuple type",
      Ty::Con(_, _) => "a constructor type",
      Ty::Fn(_, _) => "a function type",
    }
  }
}

pub(crate) type RecordTy = BTreeMap<sml_hir::Lab, Ty>;

/// Definition: `TyName`
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Sym(idx::Idx);

impl fmt::Debug for Sym {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let mut dt = f.debug_tuple("Sym");
    match self.special() {
      None => dt.field(&self.0),
      Some(x) => dt.field(&x),
    };
    dt.finish()
  }
}

macro_rules! mk_special_syms {
  ($( ($idx:expr, $mk_ty:ident, $name:ident, $str:literal), )*) => {
    impl Sym {
      $(
        pub(crate) const $name: Self = Self(idx::Idx::new_u32($idx));
      )*

      pub(crate) fn special(&self) -> Option<&'static str> {
        let s = match *self {
          $(
            Self::$name => $str,
          )*
          _ => return None,
        };
        Some(s)
      }
    }

    impl Ty {
      $(
        mk_special_syms!(@mk_ty, $mk_ty, $name, $idx);
      )*
    }
  };
  (@mk_ty, y, $name:ident, $idx:expr) => {
    pub(crate) const $name: Ty = Ty::zero(Sym::$name);
  };
  (@mk_ty, n, $name:ident, $idx:expr) => {};
}

// @sync(special_sym_order)
mk_special_syms![
  (0, y, EXN, "exn"),
  (1, y, INT, "int"),
  (2, y, WORD, "word"),
  (3, y, REAL, "real"),
  (4, y, CHAR, "char"),
  (5, y, STRING, "string"),
  (6, y, BOOL, "bool"),
  (7, n, LIST, "list"),
  (8, n, REF, "ref"),
];

impl Sym {
  /// there's only 1, and it's EXN.
  const NUM_WEIRD: usize = 1;

  /// never call this on a weird sym.
  fn idx(self) -> usize {
    self.0.to_usize() - Self::NUM_WEIRD
  }

  /// Returns whether this `Sym` was generated by a [`Syms`] after that `Syms` generated the
  /// `marker`.
  pub(crate) fn generated_after(self, marker: SymsMarker) -> bool {
    self != Self::EXN && self.idx() >= marker.0
  }
}

/// Information about generated types, generated exceptions, and overload types.
///
/// Note the `Default` impl is "fake", in that it returns a totally empty `Syms`, which will lack
/// even built-in items like `type int` and `exception Bind`.
#[derive(Debug, Default, Clone)]
pub struct Syms {
  /// remember: always use Sym::idx to index
  store: Vec<(sml_hir::Path, TyInfo)>,
  exns: Vec<(sml_hir::Path, Option<Ty>)>,
  overloads: Overloads,
}

impl Syms {
  pub(crate) fn start(&mut self, name: sml_hir::Path) -> StartedSym {
    let ty_info =
      TyInfo { ty_scheme: TyScheme::zero(Ty::None), val_env: ValEnv::default(), def: None };
    self.store.push((name, ty_info));
    StartedSym {
      bomb: DropBomb::new("must be passed to Syms::finish"),
      // calculate len after push, because we sub 1 in get, because of Sym::EXN.
      sym: Sym(idx::Idx::new(self.store.len())),
    }
  }

  pub(crate) fn finish(&mut self, mut started: StartedSym, ty_info: TyInfo) {
    started.bomb.defuse();
    self.store[started.sym.idx()].1 = ty_info;
  }

  /// Returns `None` iff passed `Sym::EXN`.
  pub(crate) fn get(&self, sym: Sym) -> Option<(&sml_hir::Path, &TyInfo)> {
    if sym == Sym::EXN {
      return None;
    }
    let &(ref name, ref info) = self.store.get(sym.idx()).unwrap();
    Some((name, info))
  }

  pub(crate) fn insert_exn(&mut self, name: sml_hir::Path, param: Option<Ty>) -> Exn {
    let ret = Exn(idx::Idx::new(self.exns.len()));
    self.exns.push((name, param));
    ret
  }

  pub(crate) fn get_exn(&self, exn: Exn) -> (&sml_hir::Path, Option<&Ty>) {
    let &(ref name, ref param) = self.exns.get(exn.0.to_usize()).unwrap();
    (name, param.as_ref())
  }

  pub(crate) fn mark(&self) -> SymsMarker {
    SymsMarker(self.store.len())
  }

  pub(crate) fn iter(&self) -> impl Iterator<Item = (&sml_hir::Path, &TyInfo)> {
    self.store.iter().map(|&(ref a, ref b)| (a, b))
  }

  pub(crate) fn overloads(&self) -> &Overloads {
    &self.overloads
  }

  pub(crate) fn overloads_mut(&mut self) -> &mut Overloads {
    &mut self.overloads
  }
}

/// Information about overloads.
#[derive(Debug, Default, Clone)]
pub(crate) struct Overloads {
  pub(crate) int: Vec<Sym>,
  pub(crate) real: Vec<Sym>,
  pub(crate) word: Vec<Sym>,
  pub(crate) string: Vec<Sym>,
  pub(crate) char: Vec<Sym>,
}

impl std::ops::Index<BasicOverload> for Overloads {
  type Output = Vec<Sym>;

  fn index(&self, index: BasicOverload) -> &Self::Output {
    match index {
      BasicOverload::Int => &self.int,
      BasicOverload::Real => &self.real,
      BasicOverload::Word => &self.word,
      BasicOverload::String => &self.string,
      BasicOverload::Char => &self.char,
    }
  }
}

impl std::ops::IndexMut<BasicOverload> for Overloads {
  fn index_mut(&mut self, index: BasicOverload) -> &mut Self::Output {
    match index {
      BasicOverload::Int => &mut self.int,
      BasicOverload::Real => &mut self.real,
      BasicOverload::Word => &mut self.word,
      BasicOverload::String => &mut self.string,
      BasicOverload::Char => &mut self.char,
    }
  }
}
/// A marker to determine when a `Sym` was generated.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SymsMarker(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Exn(idx::Idx);

/// A helper to construct information about [`Syms`]s.
pub(crate) struct StartedSym {
  bomb: DropBomb,
  sym: Sym,
}

impl StartedSym {
  pub(crate) fn sym(&self) -> Sym {
    self.sym
  }
}

/// Definition: `TypeScheme`, `TypeFcn`
#[derive(Debug, Clone)]
pub(crate) struct TyScheme {
  pub(crate) bound_vars: BoundTyVars,
  pub(crate) ty: Ty,
}

impl TyScheme {
  /// zero as in this type scheme binds zero variables.
  pub(crate) fn zero(ty: Ty) -> Self {
    Self { bound_vars: BoundTyVars::default(), ty }
  }

  /// one as in this type scheme binds one variable.
  pub(crate) fn one<F>(f: F) -> Self
  where
    F: FnOnce(Ty) -> (Ty, Option<TyVarKind>),
  {
    let mut bound_vars = BoundTyVars::new();
    let mut ty = None::<Ty>;
    BoundTyVar::add_to_binder(&mut bound_vars, |x| {
      let res = f(Ty::BoundVar(x));
      ty = Some(res.0);
      res.1
    });
    Self { bound_vars, ty: ty.unwrap() }
  }

  pub(crate) fn n_ary<I>(iter: I, sym: Sym) -> Self
  where
    I: Iterator<Item = Option<TyVarKind>>,
  {
    let bound_vars: BoundTyVars = iter.collect();
    let ty =
      Ty::Con(BoundTyVar::iter_for(bound_vars.iter()).map(|(x, _)| Ty::BoundVar(x)).collect(), sym);
    Self { bound_vars, ty }
  }
}

#[derive(Debug, Clone)]
pub(crate) enum TyVarKind {
  Equality,
  Overloaded(Overload),
  Record(RecordTy),
}

/// Definition: `TyStr`
#[derive(Debug, Clone)]
pub(crate) struct TyInfo {
  pub(crate) ty_scheme: TyScheme,
  pub(crate) val_env: ValEnv,
  pub(crate) def: Option<def::Def>,
}

/// Definition: `TyEnv`
pub(crate) type TyEnv = FxHashMap<str_util::Name, TyInfo>;

/// Definition: `ValEnv`
pub(crate) type ValEnv = FxHashMap<str_util::Name, ValInfo>;

#[derive(Debug, Clone)]
pub(crate) struct ValInfo {
  pub(crate) ty_scheme: TyScheme,
  pub(crate) id_status: IdStatus,
  pub(crate) def: Option<def::Def>,
}

/// Definition: `IdStatus`
#[derive(Debug, Clone, Copy)]
pub(crate) enum IdStatus {
  Con,
  Exn(Exn),
  Val,
}

impl IdStatus {
  pub(crate) fn same_kind_as(self, other: Self) -> bool {
    matches!(
      (self, other),
      (Self::Con, Self::Con) | (Self::Exn(_), Self::Exn(_)) | (Self::Val, Self::Val)
    )
  }
}

/// Information about meta type variables.
#[derive(Debug, Default, Clone)]
pub struct MetaVarInfo(FxHashMap<MetaTyVar, TyVarKind>);

impl MetaVarInfo {
  pub(crate) fn get(&self, mv: MetaTyVar) -> Option<&TyVarKind> {
    self.0.get(&mv)
  }
}

#[derive(Debug, Default)]
pub(crate) struct Subst {
  mv_info: MetaVarInfo,
  entries: FxHashMap<MetaTyVar, SubstEntry>,
}

impl Subst {
  pub(crate) fn insert(&mut self, mv: MetaTyVar, entry: SubstEntry) -> Option<SubstEntry> {
    match &entry {
      SubstEntry::Solved(_) => {}
      SubstEntry::Kind(kind) => {
        self.mv_info.0.insert(mv, kind.clone());
      }
    }
    self.entries.insert(mv, entry)
  }

  pub(crate) fn get(&self, mv: MetaTyVar) -> Option<&SubstEntry> {
    self.entries.get(&mv)
  }

  pub(crate) fn into_meta_var_info(self) -> MetaVarInfo {
    self.mv_info
  }
}

#[derive(Debug, Clone)]
pub(crate) enum SubstEntry {
  /// We solved this meta var to a `Ty`.
  Solved(Ty),
  /// This meta var is a special type variable, into which only certain kinds of types can be
  /// substituted.
  Kind(TyVarKind),
}

/// Used to be a newtype, but we ended up wanting to use many fundamental vec operations.
pub(crate) type BoundTyVars = Vec<Option<TyVarKind>>;

/// Used to be a newtype, but we ended up wanting to use many fundamental map operations.
pub(crate) type FixedTyVars = BTreeMap<FixedTyVar, Option<BoundTyVar>>;
