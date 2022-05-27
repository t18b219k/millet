use crate::error::ErrorKind;
use crate::pat_match::Pat;
use crate::st::St;
use crate::types::{generalize, Cx, Env, FixedTyVars, Ty, ValEnv};
use crate::unify::unify;
use crate::util::apply;
use crate::{exp, pat};

pub(crate) fn get(st: &mut St, cx: &Cx, ars: &hir::Arenas, env: &mut Env, dec: hir::DecIdx) {
  match &ars.dec[dec] {
    hir::Dec::Val(ty_vars, val_binds) => {
      let mut cx = cx.clone();
      let fixed_vars = add_fixed_ty_vars(st, &mut cx, ty_vars);
      // we actually resort to indexing logic because this is a little weird:
      // - we represent the recursive nature of ValBinds (and all other things that recurse with
      //   `and`) as a sequence of non-recursive items.
      // - if a ValBind is `rec`, it's not just this one, it's all the rest of them.
      // - we need to go over the recursive ValBinds twice.
      let mut idx = 0usize;
      let mut ve = ValEnv::default();
      while let Some(val_bind) = val_binds.get(idx) {
        if val_bind.rec {
          // this and all other remaining ones are recursive.
          break;
        }
        idx += 1;
        let (pm_pat, want) = pat::get(st, &cx, ars, &mut ve, val_bind.pat);
        get_val_exp(st, &cx, ars, val_bind.exp, pm_pat, want);
      }
      // deal with the recursive ones. first do all the patterns so we can update the val env. we
      // also need a separate recursive-only val env.
      let mut rec_ve = ValEnv::default();
      let got_pats: Vec<_> = val_binds[idx..]
        .iter()
        .map(|val_bind| pat::get(st, &cx, ars, &mut rec_ve, val_bind.pat))
        .collect();
      // merge the recursive and non-recursive val envs, making sure they don't clash.
      for (name, val_info) in rec_ve.iter() {
        if ve.insert(name.clone(), val_info.clone()).is_some() {
          st.err(ErrorKind::Redefined);
        }
      }
      // extend the cx with only the recursive val env.
      cx.env.val_env.extend(rec_ve);
      for (val_bind, (pm_pat, want)) in val_binds[idx..].iter().zip(got_pats) {
        if !matches!(ars.exp[val_bind.exp], hir::Exp::Fn(_)) {
          st.err(ErrorKind::ValRecExpNotFn);
        }
        get_val_exp(st, &cx, ars, val_bind.exp, pm_pat, want);
      }
      // generalize the entire merged val env.
      for val_info in ve.values_mut() {
        generalize(st.subst(), fixed_vars.clone(), &mut val_info.ty_scheme);
      }
      // extend the overall env with that.
      env.val_env.extend(ve);
    }
    hir::Dec::Ty(_) => {
      // TODO
    }
    hir::Dec::Datatype(_) => {
      // TODO
    }
    hir::Dec::DatatypeCopy(_, _) => {
      // TODO
    }
    hir::Dec::Abstype(_, _) => {
      // TODO
    }
    hir::Dec::Exception(_) => {
      // TODO
    }
    hir::Dec::Local(_, _) => {
      // TODO
    }
    hir::Dec::Open(_) => {
      // TODO
    }
    hir::Dec::Seq(decs) => {
      for &dec in decs {
        get(st, cx, ars, env, dec);
      }
    }
  }
}

fn add_fixed_ty_vars(st: &mut St, cx: &mut Cx, ty_vars: &[hir::TyVar]) -> FixedTyVars {
  let mut fixed_vars = FixedTyVars::default();
  for ty_var in ty_vars.iter() {
    let fv = st.gen_fixed_var(ty_var.clone());
    // TODO shadowing? scoping?
    cx.ty_vars.insert(ty_var.clone(), fv.clone());
    fixed_vars.insert(fv);
  }
  fixed_vars
}

fn get_val_exp(
  st: &mut St,
  cx: &Cx,
  ars: &hir::Arenas,
  exp: hir::ExpIdx,
  pm_pat: Pat,
  mut want: Ty,
) {
  let got = exp::get(st, cx, ars, exp);
  unify(st, want.clone(), got);
  apply(st.subst(), &mut want);
  pat::get_match(
    st,
    vec![pm_pat],
    want,
    Some(ErrorKind::NonExhaustiveBinding),
  );
}
