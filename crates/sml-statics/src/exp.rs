use crate::config::Cfg;
use crate::error::{ErrorKind, Item};
use crate::get_env::get_val_info;
use crate::info::TyEntry;
use crate::pat_match::Pat;
use crate::st::St;
use crate::types::{Cx, Def, Env, EnvLike as _, Generalizable, SymsMarker, Ty, TyScheme, ValEnv};
use crate::unify::unify;
use crate::util::{apply, get_scon, instantiate, record};
use crate::{dec, pat, ty};

pub(crate) fn get_and_check_ty_escape(
  st: &mut St,
  cfg: Cfg,
  cx: &Cx,
  marker: &SymsMarker,
  ars: &sml_hir::Arenas,
  exp: sml_hir::ExpIdx,
) -> Ty {
  let ret = get(st, cfg, cx, ars, exp);
  if let (Some(exp), Some(ty)) = (exp, ty_escape(cx, marker, &ret)) {
    st.err(exp, ErrorKind::TyEscape(ty));
  }
  ret
}

fn get(st: &mut St, cfg: Cfg, cx: &Cx, ars: &sml_hir::Arenas, exp: sml_hir::ExpIdx) -> Ty {
  let exp = match exp {
    Some(x) => x,
    None => return Ty::None,
  };
  // NOTE: do not early return, since we add to the Info at the bottom.
  let mut ty_scheme = None::<TyScheme>;
  let mut definition = None::<Def>;
  let ret = match &ars.exp[exp] {
    sml_hir::Exp::Hole => {
      let mv = st.meta_gen.gen(Generalizable::Always);
      st.insert_hole(mv, exp.into());
      Ty::MetaVar(mv)
    }
    // @def(1)
    sml_hir::Exp::SCon(scon) => get_scon(st, Generalizable::Always, scon),
    // @def(2)
    sml_hir::Exp::Path(path) => match get_val_info(&cx.env, path) {
      Ok(Some(val_info)) => {
        ty_scheme = Some(val_info.ty_scheme.clone());
        definition = val_info.def;
        if let Some(def) = val_info.def {
          st.mark_used(def.idx);
        }
        instantiate(st, Generalizable::Always, val_info.ty_scheme.clone())
      }
      Ok(None) => {
        st.err(exp, ErrorKind::Undefined(Item::Val, path.last().clone()));
        Ty::None
      }
      Err(e) => {
        st.err(exp, e);
        Ty::None
      }
    },
    // @def(3)
    sml_hir::Exp::Record(rows) => {
      let rows = record(st, rows, exp.into(), |st, _, exp| get(st, cfg, cx, ars, exp));
      Ty::Record(rows)
    }
    // @def(4)
    sml_hir::Exp::Let(dec, inner) => {
      let mut let_env = Env::default();
      dec::get(st, cfg, cx, ars, &mut let_env, *dec);
      let mut cx = cx.clone();
      cx.env.append(&mut let_env);
      get(st, cfg, &cx, ars, *inner)
    }
    // @def(8)
    sml_hir::Exp::App(func, argument) => {
      let func_ty = get(st, cfg, cx, ars, *func);
      let arg_ty = get(st, cfg, cx, ars, *argument);
      // we could choose to not `match` on `func_ty` and just use the `MetaVar` case always and it
      // would still be correct. however, matching on `func_ty` lets us emit slightly better error
      // messages sometimes.
      match func_ty {
        Ty::None => Ty::None,
        Ty::BoundVar(_) => unreachable!("bound vars should be instantiated"),
        Ty::MetaVar(_) => {
          let mut ret = Ty::MetaVar(st.meta_gen.gen(Generalizable::Always));
          let got = Ty::fun(arg_ty, ret.clone());
          unify(st, func_ty, got, exp.into());
          apply(st.subst(), &mut ret);
          ret
        }
        Ty::FixedVar(_) | Ty::Record(_) | Ty::Con(_, _) => {
          st.err(func.unwrap_or(exp), ErrorKind::AppLhsNotFn(func_ty));
          Ty::None
        }
        Ty::Fn(want_arg, mut want_res) => {
          unify(st, *want_arg, arg_ty, argument.unwrap_or(exp).into());
          apply(st.subst(), want_res.as_mut());
          *want_res
        }
      }
    }
    // @def(10)
    sml_hir::Exp::Handle(inner, matcher) => {
      let mut exp_ty = get(st, cfg, cx, ars, *inner);
      let (pats, param, res) = get_matcher(st, cfg, cx, ars, matcher, exp.into());
      let idx = inner.unwrap_or(exp);
      unify(st, Ty::EXN, param.clone(), idx.into());
      unify(st, exp_ty.clone(), res, idx.into());
      apply(st.subst(), &mut exp_ty);
      st.insert_handle(pats, param, idx.into());
      exp_ty
    }
    // @def(11)
    sml_hir::Exp::Raise(inner) => {
      let got = get(st, cfg, cx, ars, *inner);
      unify(st, Ty::EXN, got, inner.unwrap_or(exp).into());
      Ty::MetaVar(st.meta_gen.gen(Generalizable::Always))
    }
    // @def(12)
    sml_hir::Exp::Fn(matcher) => {
      let (pats, param, res) = get_matcher(st, cfg, cx, ars, matcher, exp.into());
      st.insert_case(pats, param.clone(), exp.into());
      Ty::fun(param, res)
    }
    // @def(9)
    sml_hir::Exp::Typed(inner, want) => {
      let got = get(st, cfg, cx, ars, *inner);
      let mut want = ty::get(st, cx, ars, ty::Mode::Regular, *want);
      unify(st, want.clone(), got, exp.into());
      apply(st.subst(), &mut want);
      want
    }
  };
  let ty_entry = TyEntry { ty: ret.clone(), ty_scheme };
  st.info().insert(exp.into(), Some(ty_entry), definition);
  ret
}

/// @def(13)
fn get_matcher(
  st: &mut St,
  cfg: Cfg,
  cx: &Cx,
  ars: &sml_hir::Arenas,
  matcher: &[(sml_hir::PatIdx, sml_hir::ExpIdx)],
  idx: sml_hir::Idx,
) -> (Vec<Pat>, Ty, Ty) {
  let mut param_ty = Ty::MetaVar(st.meta_gen.gen(Generalizable::Always));
  let mut res_ty = Ty::MetaVar(st.meta_gen.gen(Generalizable::Always));
  let mut pats = Vec::<Pat>::new();
  st.meta_gen.inc_rank();
  // @def(14)
  for &(pat, exp) in matcher {
    let mut ve = ValEnv::default();
    let cfg = pat::Cfg { cfg, gen: Generalizable::Sometimes, rec: false };
    let (pm_pat, pat_ty) = pat::get(st, cfg, ars, cx, &mut ve, pat);
    let mut cx = cx.clone();
    cx.env.push(Env { val_env: ve, ..Default::default() });
    let exp_ty = get(st, cfg.cfg, &cx, ars, exp);
    let pi = pat.map_or(idx, Into::into);
    unify(st, param_ty.clone(), pat_ty, pi);
    let ei = exp.map_or(idx, Into::into);
    unify(st, res_ty.clone(), exp_ty, ei);
    apply(st.subst(), &mut param_ty);
    apply(st.subst(), &mut res_ty);
    pats.push(pm_pat);
  }
  st.meta_gen.dec_rank();
  (pats, param_ty, res_ty)
}

fn ty_escape(cx: &Cx, m: &SymsMarker, ty: &Ty) -> Option<Ty> {
  match ty {
    Ty::None | Ty::BoundVar(_) | Ty::MetaVar(_) => None,
    Ty::FixedVar(fv) => (!cx.fixed.contains_key(fv.ty_var())).then(|| ty.clone()),
    Ty::Record(rows) => rows.values().find_map(|ty| ty_escape(cx, m, ty)),
    Ty::Con(args, sym) => sym
      .generated_after(m)
      .then(|| ty.clone())
      .or_else(|| args.iter().find_map(|ty| ty_escape(cx, m, ty))),
    Ty::Fn(param, res) => ty_escape(cx, m, param).or_else(|| ty_escape(cx, m, res)),
  }
}
