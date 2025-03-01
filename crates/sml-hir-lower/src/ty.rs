//! Lowering types.

use crate::common::{ck_trailing, forbid_opaque_asc, get_lab, get_path};
use crate::util::{ErrorKind, Sep, St};
use sml_syntax::ast::{self, AstNode as _, SyntaxNodePtr};

pub(crate) fn get(st: &mut St<'_>, ty: Option<ast::Ty>) -> sml_hir::TyIdx {
  let ty = ty?;
  let ptr = SyntaxNodePtr::new(ty.syntax());
  let ret = match ty {
    ast::Ty::HoleTy(_) | ast::Ty::WildcardTy(_) => sml_hir::Ty::Hole,
    ast::Ty::TyVarTy(ty) => sml_hir::Ty::Var(sml_hir::TyVar::new(ty.ty_var()?.text())),
    ast::Ty::RecordTy(ty) => {
      ck_trailing(st, Sep::Comma, ty.ty_rows().map(|x| x.comma()));
      let rows = ty.ty_rows().filter_map(|row| {
        forbid_opaque_asc(st, row.ascription());
        let lab = get_lab(st, row.lab()?);
        let ty = get(st, row.ty());
        Some((lab, ty))
      });
      sml_hir::Ty::Record(rows.collect())
    }
    ast::Ty::ConTy(ty) => {
      let path = get_path(ty.path()?)?;
      let separators = ty.ty_seq().into_iter().flat_map(|x| x.ty_args()).map(|x| x.comma());
      ck_trailing(st, Sep::Comma, separators);
      let ty_args = ty.ty_seq().into_iter().flat_map(|x| x.ty_args()).map(|x| get(st, x.ty()));
      sml_hir::Ty::Con(ty_args.collect(), path)
    }
    ast::Ty::OneArgConTy(ty) => {
      let path = get_path(ty.path()?)?;
      sml_hir::Ty::Con(vec![get(st, ty.ty())], path)
    }
    ast::Ty::TupleTy(ty) => {
      let rows = std::iter::once(ty.ty())
        .chain(ty.star_tys().map(|x| x.ty()))
        .enumerate()
        .map(|(idx, t)| (sml_hir::Lab::tuple(idx), get(st, t)));
      sml_hir::Ty::Record(rows.collect())
    }
    ast::Ty::FnTy(ty) => sml_hir::Ty::Fn(get(st, ty.param()), get(st, ty.res())),
    // @def(48)
    ast::Ty::ParenTy(ty) => {
      let inner = ty.ty();
      if inner.as_ref().map_or(false, warn_unnecessary_parens) {
        st.err(ty.syntax().text_range(), ErrorKind::UnnecessaryParens);
      }
      return get(st, inner);
    }
  };
  st.ty(ret, ptr)
}

/// not necessarily "is atomic".
fn warn_unnecessary_parens(ty: &ast::Ty) -> bool {
  match ty {
    ast::Ty::TyVarTy(_) | ast::Ty::RecordTy(_) | ast::Ty::ParenTy(_) => true,
    ast::Ty::ConTy(ty) => ty.ty_seq().is_none(),
    ast::Ty::HoleTy(_)
    | ast::Ty::WildcardTy(_)
    | ast::Ty::OneArgConTy(_)
    | ast::Ty::TupleTy(_)
    | ast::Ty::FnTy(_) => false,
  }
}

pub(crate) fn var_seq(st: &mut St<'_>, tvs: Option<ast::TyVarSeq>) -> Vec<sml_hir::TyVar> {
  let separators = tvs.iter().flat_map(ast::TyVarSeq::ty_var_args).map(|x| x.comma());
  ck_trailing(st, Sep::Comma, separators);
  tvs
    .into_iter()
    .flat_map(|x| x.ty_var_args())
    .filter_map(|x| x.ty_var())
    .map(|tok| sml_hir::TyVar::new(tok.text()))
    .collect()
}
