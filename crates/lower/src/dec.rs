use crate::common::{get_name, get_path};
use crate::util::Cx;
use crate::{exp, pat, ty};
use syntax::ast;

pub(crate) fn get(cx: &mut Cx, dec: Option<ast::DecSeq>) -> hir::DecIdx {
  let mut decs: Vec<_> = dec
    .into_iter()
    .flat_map(|x| x.dec_in_seqs())
    .filter_map(|x| x.dec())
    .map(|dec| {
      let res = get_one(cx, dec);
      cx.arenas.dec.alloc(res)
    })
    .collect();
  if decs.len() == 1 {
    decs.pop().unwrap()
  } else {
    cx.arenas.dec.alloc(hir::Dec::Seq(decs))
  }
}

fn get_one(cx: &mut Cx, dec: ast::Dec) -> hir::Dec {
  match dec {
    ast::Dec::ValDec(dec) => {
      let ty_vars = ty_var_seq(dec.ty_var_seq());
      let binds: Vec<_> = dec
        .val_binds()
        .map(|x| hir::ValBind {
          rec: x.rec_kw().is_some(),
          pat: pat::get(cx, x.pat()),
          exp: exp::get(cx, x.exp()),
        })
        .collect();
      hir::Dec::Val(ty_vars, binds)
    }
    ast::Dec::FunDec(dec) => {
      let ty_vars = ty_var_seq(dec.ty_var_seq());
      let val_binds: Vec<_> = dec
        .fun_binds()
        .map(|fun_bind| {
          let mut name = None::<syntax::SyntaxToken>;
          let mut num_pats = None::<usize>;
          let arms: Vec<_> = fun_bind
            .fun_bind_cases()
            .map(|case| {
              let mut pats = Vec::<hir::PatIdx>::with_capacity(2);
              let head_name = case.fun_bind_case_head().and_then(|head| match head {
                ast::FunBindCaseHead::PrefixFunBindCaseHead(head) => head.name(),
                ast::FunBindCaseHead::InfixFunBindCaseHead(head) => {
                  pats.push(pat::get(cx, head.lhs()));
                  pats.push(pat::get(cx, head.rhs()));
                  head.name()
                }
              });
              match (name.as_ref(), head_name) {
                (_, None) => {}
                (None, Some(head_name)) => name = Some(head_name),
                (Some(name), Some(head_name)) => {
                  if name.text() != head_name.text() {
                    // TODO error
                  }
                }
              }
              pats.extend(case.pats().map(|x| pat::get(cx, Some(x))));
              match num_pats {
                None => num_pats = Some(pats.len()),
                Some(num_pats) => {
                  if num_pats != pats.len() {
                    // TODO error
                  }
                }
              }
              let pat = cx.arenas.pat.alloc(pat::tuple(pats));
              let ty = case.ty_annotation().map(|x| ty::get(cx, x.ty()));
              let mut exp = exp::get(cx, case.exp());
              if let Some(ty) = ty {
                exp = cx.arenas.exp.alloc(hir::Exp::Typed(exp, ty));
              }
              (pat, exp)
            })
            .collect();
          let pat = name.map_or(hir::Pat::None, |x| pat::name(x.text()));
          let arg_names: Vec<_> = (0..num_pats.unwrap_or(1)).map(|_| cx.fresh()).collect();
          let head = exp::tuple(
            arg_names
              .iter()
              .map(|x| cx.arenas.exp.alloc(exp::name(x.as_str()))),
          );
          let head = cx.arenas.exp.alloc(head);
          let case = exp::case_exp(cx, head, arms);
          hir::ValBind {
            rec: true,
            pat: cx.arenas.pat.alloc(pat),
            exp: arg_names
              .into_iter()
              .rev()
              .fold(cx.arenas.exp.alloc(case), |body, name| {
                let pat = cx.arenas.pat.alloc(pat::name(name.as_str()));
                cx.arenas.exp.alloc(hir::Exp::Fn(vec![(pat, body)]))
              }),
          }
        })
        .collect();
      hir::Dec::Val(ty_vars, val_binds)
    }
    ast::Dec::TyDec(dec) => ty_binds(cx, dec.ty_binds()),
    ast::Dec::DatDec(dec) => {
      let mut ret = hir::Dec::Datatype(dat_binds(cx, dec.dat_binds()));
      if let Some(with_ty) = dec.with_type() {
        let ty_dec = ty_binds(cx, with_ty.ty_binds());
        ret = hir::Dec::Seq(vec![cx.arenas.dec.alloc(ret), cx.arenas.dec.alloc(ty_dec)]);
      }
      ret
    }
    ast::Dec::DatCopyDec(dec) => {
      let datatype_copy = get_name(dec.name()).and_then(|name| {
        dec
          .path()
          .and_then(get_path)
          .map(|path| hir::Dec::DatatypeCopy(name, path))
      });
      // HACK: relying on the fact that an empty seq has no effect
      datatype_copy.unwrap_or(hir::Dec::Seq(Vec::new()))
    }
    ast::Dec::AbstypeDec(dec) => {
      let dbs = dat_binds(cx, dec.dat_binds());
      let ty_dec = dec.with_type().map(|x| ty_binds(cx, x.ty_binds()));
      let mut d = get(cx, dec.dec_seq());
      if let Some(ty_dec) = ty_dec {
        let ty_dec = cx.arenas.dec.alloc(ty_dec);
        d = cx.arenas.dec.alloc(hir::Dec::Seq(vec![d, ty_dec]));
      }
      // TODO: "see note in text"
      hir::Dec::Abstype(dbs, d)
    }
    ast::Dec::ExDec(dec) => hir::Dec::Exception(
      dec
        .ex_binds()
        .filter_map(|x| {
          let name = get_name(x.name())?;
          let ret = match x.ex_bind_inner()? {
            ast::ExBindInner::OfTy(x) => {
              hir::ExBind::New(name, x.ty().map(|x| ty::get(cx, Some(x))))
            }
            ast::ExBindInner::EqPath(x) => hir::ExBind::Copy(name, get_path(x.path()?)?),
          };
          Some(ret)
        })
        .collect(),
    ),
    ast::Dec::LocalDec(_) => todo!(),
    ast::Dec::OpenDec(_) => todo!(),
    ast::Dec::InfixDec(_) => todo!(),
    ast::Dec::InfixrDec(_) => todo!(),
    ast::Dec::NonfixDec(_) => todo!(),
  }
}

fn dat_binds<I>(cx: &mut Cx, iter: I) -> Vec<hir::DatBind>
where
  I: Iterator<Item = ast::DatBind>,
{
  iter
    .filter_map(|dat_bind| {
      let name = get_name(dat_bind.name())?;
      Some(hir::DatBind {
        ty_vars: ty_var_seq(dat_bind.ty_var_seq()),
        name,
        cons: dat_bind
          .con_binds()
          .filter_map(|con_bind| {
            let name = get_name(con_bind.name())?;
            let ty = con_bind.of_ty().map(|x| ty::get(cx, x.ty()));
            Some((name, ty))
          })
          .collect(),
      })
    })
    .collect()
}

fn ty_binds<I>(cx: &mut Cx, iter: I) -> hir::Dec
where
  I: Iterator<Item = ast::TyBind>,
{
  hir::Dec::Ty(
    iter
      .filter_map(|x| {
        let name = get_name(x.name())?;
        Some(hir::TyBind {
          ty_vars: ty_var_seq(x.ty_var_seq()),
          name,
          ty: ty::get(cx, x.ty()),
        })
      })
      .collect(),
  )
}

fn ty_var_seq(tvs: Option<ast::TyVarSeq>) -> Vec<hir::TyVar> {
  tvs
    .into_iter()
    .flat_map(|x| x.ty_var_args())
    .filter_map(|x| x.ty_var())
    .map(|tok| hir::TyVar::new(tok.text()))
    .collect()
}
