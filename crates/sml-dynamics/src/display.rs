//! Displaying some types.

// TODO fix prec everywhere

#![allow(clippy::too_many_lines)]

use crate::dynamics::Dynamics;
use crate::types::{Con, ConKind, Env, Exception, FrameKind, Step, Val};
use sml_hir::Lab;
use std::fmt;

impl fmt::Display for Dynamics<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let ars = &self.cx.ars;
    f.write_str("(* env:\n")?;
    if let Some(frame) = self.st.frames.last() {
      EnvDisplay { env: &frame.env, ars, indent: 0 }.fmt(f)?;
    }
    f.write_str("*)\n")?;
    let mut indent = 0usize;
    // TODO set this with the frames?
    let prec = Prec::Min;
    for frame in &self.st.frames {
      match &frame.kind {
        FrameKind::AppFunc(_) | FrameKind::Handle(_) => {}
        FrameKind::Record(vs, lab, _) => {
          f.write_str("{ ")?;
          for (lab, val) in vs {
            ValRowDisplay { lab, val, ars }.fmt(f)?;
            f.write_str(", ")?;
          }
          lab.fmt(f)?;
          f.write_str(" = ")?;
        }
        FrameKind::AppClosureArg(matcher) => {
          f.write_str("(")?;
          FnDisplay { matcher, ars }.fmt(f)?;
          f.write_str(") (")?;
        }
        FrameKind::AppConArg(kind) => {
          kind.fmt(f)?;
          f.write_str(" (")?;
        }
        FrameKind::Raise => f.write_str("raise ")?,
        FrameKind::Let(_, _) => {
          f.write_str("let")?;
          indent += 1;
          write_nl_indent(indent, f)?;
        }
        FrameKind::ValBind(recursive, pat, _) => {
          f.write_str("val ")?;
          if *recursive {
            f.write_str("rec ")?;
          }
          PatDisplay { pat: pat.ok_or(fmt::Error)?, ars }.fmt(f)?;
          f.write_str(" = ")?;
        }
        FrameKind::Local(_, _) => {
          f.write_str("local")?;
          indent += 1;
          write_nl_indent(indent, f)?;
        }
        FrameKind::In(_) => {
          f.write_str("local in")?;
          indent += 1;
          write_nl_indent(indent, f)?;
        }
      }
    }
    f.write_str("(* >> *) ")?;
    match self.step.as_ref().ok_or(fmt::Error)? {
      Step::Exp(exp) => ExpDisplay { exp: *exp, ars, prec }.fmt(f)?,
      Step::Val(val) => ValDisplay { val, ars, prec }.fmt(f)?,
      Step::Raise(exception) => {
        f.write_str("raise ")?;
        ExceptionDisplay { exception, ars }.fmt(f)?;
      }
      Step::Dec(dec) => DecDisplay { dec: *dec, ars }.fmt(f)?,
      Step::DecDone => {}
    }
    f.write_str(" (* << *)")?;
    for frame in self.st.frames.iter().rev() {
      match &frame.kind {
        FrameKind::Raise => {}
        FrameKind::Record(_, _, es) => {
          f.write_str(", ")?;
          fmt_util::comma_seq(f, es.iter().map(|&(ref lab, exp)| ExpRowDisplay { lab, exp, ars }))?;
          f.write_str(" }")?;
        }
        FrameKind::AppClosureArg(_) | FrameKind::AppConArg(_) => f.write_str(")")?,
        FrameKind::AppFunc(exp) => {
          f.write_str(" ")?;
          ExpDisplay { exp: exp.ok_or(fmt::Error)?, ars, prec: Prec::Atomic }.fmt(f)?;
        }
        FrameKind::Handle(matcher) => {
          f.write_str(" handle ")?;
          fmt_util::sep_seq(f, " | ", matcher.iter().map(|arm| ArmDisplay { arm, ars }))?;
        }
        FrameKind::Let(decs, exp) => {
          for &dec in decs.iter().rev() {
            write_nl_indent(indent, f)?;
            DecDisplay { dec, ars }.fmt(f)?;
          }
          write_nl_indent(indent - 1, f)?;
          f.write_str("in")?;
          write_nl_indent(indent, f)?;
          ExpDisplay { exp: exp.ok_or(fmt::Error)?, ars, prec: Prec::Min }.fmt(f)?;
          indent -= 1;
          write_nl_indent(indent, f)?;
          f.write_str("end")?;
        }
        FrameKind::ValBind(_, _, val_binds) => {
          for &val_bind in val_binds.iter().rev() {
            write_nl_indent(indent, f)?;
            ValBindDisplay { val_bind, ars }.fmt(f)?;
          }
        }
        FrameKind::Local(local_decs, in_decs) => {
          for &dec in local_decs.iter().rev() {
            write_nl_indent(indent, f)?;
            DecDisplay { dec, ars }.fmt(f)?;
          }
          write_nl_indent(indent - 1, f)?;
          f.write_str("in")?;
          for &dec in in_decs.iter().rev() {
            write_nl_indent(indent, f)?;
            DecDisplay { dec, ars }.fmt(f)?;
          }
          indent -= 1;
          write_nl_indent(indent, f)?;
          f.write_str("end")?;
        }
        FrameKind::In(in_decs) => {
          for &dec in in_decs.iter().rev() {
            write_nl_indent(indent, f)?;
            DecDisplay { dec, ars }.fmt(f)?;
          }
          indent -= 1;
          write_nl_indent(indent, f)?;
          f.write_str("end")?;
        }
      }
    }
    Ok(())
  }
}

// TODO have this derive Ord etc and remove ad-hoc matches!()
#[derive(Debug, Clone, Copy)]
enum Prec {
  Min,
  Matcher,
  App,
  Atomic,
}

fn write_nl_indent(indent: usize, f: &mut fmt::Formatter<'_>) -> fmt::Result {
  f.write_str("\n")?;
  for _ in 0..indent {
    f.write_str("  ")?;
  }
  Ok(())
}

struct ValDisplay<'a> {
  val: &'a Val,
  ars: &'a sml_hir::Arenas,
  prec: Prec,
}

impl fmt::Display for ValDisplay<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self.val {
      Val::SCon(scon) => scon.fmt(f),
      Val::Con(con) => {
        ConDisplay { con, ars: self.ars, atomic: matches!(self.prec, Prec::Atomic) }.fmt(f)
      }
      Val::Record(vs) => {
        f.write_str("{ ")?;
        let rows = vs.iter().map(|(lab, val)| ValRowDisplay { lab, val, ars: self.ars });
        fmt_util::comma_seq(f, rows)?;
        f.write_str(" }")
      }
      Val::Closure(clos) => {
        let needs_paren = matches!(self.prec, Prec::App | Prec::Matcher | Prec::Atomic);
        if needs_paren {
          f.write_str("(")?;
        }
        FnDisplay { matcher: &clos.matcher, ars: self.ars }.fmt(f)?;
        if needs_paren {
          f.write_str(")")?;
        }
        Ok(())
      }
    }
  }
}

struct ValBindDisplay<'a> {
  val_bind: sml_hir::ValBind,
  ars: &'a sml_hir::Arenas,
}

impl fmt::Display for ValBindDisplay<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("val ")?;
    if self.val_bind.rec {
      f.write_str("rec ")?;
    }
    PatDisplay { pat: self.val_bind.pat.ok_or(fmt::Error)?, ars: self.ars }.fmt(f)?;
    f.write_str(" = ")?;
    ExpDisplay { exp: self.val_bind.exp.ok_or(fmt::Error)?, ars: self.ars, prec: Prec::Min }.fmt(f)
  }
}

struct FnDisplay<'a> {
  matcher: &'a [sml_hir::Arm],
  ars: &'a sml_hir::Arenas,
}

impl fmt::Display for FnDisplay<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("fn ")?;
    let arms = self.matcher.iter().map(|arm| ArmDisplay { arm, ars: self.ars });
    fmt_util::sep_seq(f, " | ", arms)
  }
}

struct ValRowDisplay<'a> {
  lab: &'a Lab,
  val: &'a Val,
  ars: &'a sml_hir::Arenas,
}

impl fmt::Display for ValRowDisplay<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.lab.fmt(f)?;
    f.write_str(" = ")?;
    ValDisplay { val: self.val, ars: self.ars, prec: Prec::Min }.fmt(f)
  }
}

struct ArmDisplay<'a> {
  arm: &'a sml_hir::Arm,
  ars: &'a sml_hir::Arenas,
}

impl fmt::Display for ArmDisplay<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    PatDisplay { pat: self.arm.pat.ok_or(fmt::Error)?, ars: self.ars }.fmt(f)?;
    f.write_str(" => ")?;
    ExpDisplay { exp: self.arm.exp.ok_or(fmt::Error)?, ars: self.ars, prec: Prec::Min }.fmt(f)
  }
}

struct ExpDisplay<'a> {
  exp: sml_hir::la_arena::Idx<sml_hir::Exp>,
  ars: &'a sml_hir::Arenas,
  prec: Prec,
}

impl fmt::Display for ExpDisplay<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match &self.ars.exp[self.exp] {
      sml_hir::Exp::Hole => f.write_str("_"),
      sml_hir::Exp::SCon(scon) => scon.fmt(f),
      sml_hir::Exp::Path(path) => path.fmt(f),
      sml_hir::Exp::Record(rows) => {
        f.write_str("{ ")?;
        let rows = rows.iter().map(|&(ref lab, exp)| ExpRowDisplay { lab, exp, ars: self.ars });
        fmt_util::comma_seq(f, rows)?;
        f.write_str(" }")
      }
      sml_hir::Exp::Let(decs, exp) => {
        f.write_str("let ")?;
        fmt_util::sep_seq(f, " ", decs.iter().rev().map(|&dec| DecDisplay { dec, ars: self.ars }))?;
        f.write_str(" in ")?;
        ExpDisplay { exp: exp.ok_or(fmt::Error)?, ars: self.ars, prec: Prec::Min }.fmt(f)?;
        f.write_str(" end")
      }
      sml_hir::Exp::App(func, argument) => {
        let needs_paren = matches!(self.prec, Prec::Atomic);
        if needs_paren {
          f.write_str("(")?;
        }
        ExpDisplay { exp: func.ok_or(fmt::Error)?, ars: self.ars, prec: Prec::App }.fmt(f)?;
        f.write_str(" ")?;
        ExpDisplay { exp: argument.ok_or(fmt::Error)?, ars: self.ars, prec: Prec::Atomic }
          .fmt(f)?;
        if needs_paren {
          f.write_str(")")?;
        }
        Ok(())
      }
      sml_hir::Exp::Handle(exp, matcher) => {
        let needs_paren = matches!(self.prec, Prec::App | Prec::Atomic);
        if needs_paren {
          f.write_str(")")?;
        }
        ExpDisplay { exp: exp.ok_or(fmt::Error)?, ars: self.ars, prec: Prec::Matcher }.fmt(f)?;
        f.write_str(" handle ")?;
        fmt_util::sep_seq(f, " | ", matcher.iter().map(|arm| ArmDisplay { arm, ars: self.ars }))?;
        if needs_paren {
          f.write_str(")")?;
        }
        Ok(())
      }
      sml_hir::Exp::Raise(exp) => {
        let needs_paren = matches!(self.prec, Prec::Matcher | Prec::App | Prec::Atomic);
        if needs_paren {
          f.write_str("(")?;
        }
        f.write_str("raise ")?;
        ExpDisplay { exp: exp.ok_or(fmt::Error)?, ars: self.ars, prec: Prec::Min }.fmt(f)?;
        if needs_paren {
          f.write_str("(")?;
        }
        Ok(())
      }
      sml_hir::Exp::Fn(matcher, _) => {
        let needs_paren = matches!(self.prec, Prec::Matcher | Prec::App | Prec::Atomic);
        if needs_paren {
          f.write_str("(")?;
        }
        FnDisplay { matcher, ars: self.ars }.fmt(f)?;
        if needs_paren {
          f.write_str(")")?;
        }
        Ok(())
      }
      sml_hir::Exp::Typed(exp, _) => {
        ExpDisplay { exp: exp.ok_or(fmt::Error)?, ars: self.ars, prec: self.prec }.fmt(f)
      }
    }
  }
}

struct ExpRowDisplay<'a> {
  lab: &'a Lab,
  exp: sml_hir::ExpIdx,
  ars: &'a sml_hir::Arenas,
}

impl fmt::Display for ExpRowDisplay<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.lab.fmt(f)?;
    f.write_str(" = ")?;
    ExpDisplay { exp: self.exp.ok_or(fmt::Error)?, ars: self.ars, prec: Prec::Min }.fmt(f)
  }
}

struct PatDisplay<'a> {
  pat: sml_hir::la_arena::Idx<sml_hir::Pat>,
  ars: &'a sml_hir::Arenas,
}

impl fmt::Display for PatDisplay<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match &self.ars.pat[self.pat] {
      sml_hir::Pat::Wild => f.write_str("_"),
      sml_hir::Pat::SCon(scon) => scon.fmt(f),
      sml_hir::Pat::Con(path, arg) => {
        path.last().fmt(f)?;
        if let Some(arg) = arg {
          f.write_str(" ")?;
          PatDisplay { pat: arg.ok_or(fmt::Error)?, ars: self.ars }.fmt(f)?;
        }
        Ok(())
      }
      sml_hir::Pat::Record { rows, allows_other } => {
        f.write_str("{ ")?;
        let rows = rows.iter().map(|&(ref lab, pat)| PatRowDisplay { lab, pat, ars: self.ars });
        fmt_util::comma_seq(f, rows)?;
        if *allows_other {
          f.write_str(", ...")?;
        }
        f.write_str(" }")
      }
      sml_hir::Pat::Typed(pat, _) => {
        PatDisplay { pat: pat.ok_or(fmt::Error)?, ars: self.ars }.fmt(f)
      }
      sml_hir::Pat::As(name, pat) => {
        name.fmt(f)?;
        f.write_str(" as ")?;
        PatDisplay { pat: pat.ok_or(fmt::Error)?, ars: self.ars }.fmt(f)
      }
      sml_hir::Pat::Or(or_pat) => {
        PatDisplay { pat: or_pat.first.ok_or(fmt::Error)?, ars: self.ars }.fmt(f)?;
        for pat in &or_pat.rest {
          f.write_str(" | ")?;
          PatDisplay { pat: pat.ok_or(fmt::Error)?, ars: self.ars }.fmt(f)?;
        }
        Ok(())
      }
    }
  }
}

struct PatRowDisplay<'a> {
  lab: &'a Lab,
  pat: sml_hir::PatIdx,
  ars: &'a sml_hir::Arenas,
}

impl fmt::Display for PatRowDisplay<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.lab.fmt(f)?;
    f.write_str(" = ")?;
    PatDisplay { pat: self.pat.ok_or(fmt::Error)?, ars: self.ars }.fmt(f)
  }
}

struct ConDisplay<'a> {
  con: &'a Con,
  ars: &'a sml_hir::Arenas,
  atomic: bool,
}

impl fmt::Display for ConDisplay<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let needs_paren = self.atomic && self.con.arg.is_some();
    if needs_paren {
      f.write_str("(")?;
    }
    self.con.kind.fmt(f)?;
    if let Some(val) = &self.con.arg {
      f.write_str(" ")?;
      ValDisplay { val: val.as_ref(), ars: self.ars, prec: Prec::Atomic }.fmt(f)?;
    }
    if needs_paren {
      f.write_str(")")?;
    }
    Ok(())
  }
}

impl fmt::Display for ConKind {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      ConKind::Dat(name) | ConKind::Exn(name, _) => name.fmt(f),
    }
  }
}

struct ExceptionDisplay<'a> {
  exception: &'a Exception,
  ars: &'a sml_hir::Arenas,
}

impl fmt::Display for ExceptionDisplay<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.exception.name.fmt(f)?;
    if let Some(val) = &self.exception.arg {
      f.write_str(" ")?;
      ValDisplay { val: val.as_ref(), ars: self.ars, prec: Prec::App }.fmt(f)?;
    }
    Ok(())
  }
}

struct DecDisplay<'a> {
  dec: sml_hir::DecIdx,
  ars: &'a sml_hir::Arenas,
}

impl fmt::Display for DecDisplay<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match &self.ars.dec[self.dec] {
      sml_hir::Dec::Val(_, val_binds) => {
        let val_binds =
          val_binds.iter().map(|&val_bind| ValBindDisplay { val_bind, ars: self.ars });
        fmt_util::sep_seq(f, " ", val_binds)
      }
      sml_hir::Dec::Ty(_) => f.write_str("type ..."),
      sml_hir::Dec::Datatype(_, _) | sml_hir::Dec::DatatypeCopy(_, _) => {
        f.write_str("datatype ...")
      }
      sml_hir::Dec::Abstype(_, _, _) => Err(fmt::Error),
      sml_hir::Dec::Exception(_) => f.write_str("exception ..."),
      sml_hir::Dec::Local(local_decs, in_decs) => {
        f.write_str("local ")?;
        fmt_util::sep_seq(f, " ", local_decs.iter().map(|&dec| DecDisplay { dec, ars: self.ars }))?;
        f.write_str(" in ")?;
        fmt_util::sep_seq(f, " ", in_decs.iter().map(|&dec| DecDisplay { dec, ars: self.ars }))?;
        f.write_str(" end")
      }
      sml_hir::Dec::Open(_) => f.write_str("open ..."),
    }
  }
}

struct EnvDisplay<'a> {
  env: &'a Env,
  ars: &'a sml_hir::Arenas,
  indent: usize,
}

impl fmt::Display for EnvDisplay<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for (name, env) in &self.env.str {
      for _ in 0..self.indent {
        f.write_str("  ")?;
      }
      name.fmt(f)?;
      f.write_str(":\n")?;
      EnvDisplay { env, ars: self.ars, indent: self.indent + 1 }.fmt(f)?;
    }
    for (name, val) in &self.env.val {
      for _ in 0..self.indent {
        f.write_str("  ")?;
      }
      name.fmt(f)?;
      f.write_str(": ")?;
      ValDisplay { val, ars: self.ars, prec: Prec::Min }.fmt(f)?;
      f.write_str("\n")?;
    }
    Ok(())
  }
}
