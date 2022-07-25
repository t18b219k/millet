//! Input to analysis.

use fast_hash::FxHashSet;
use paths::{PathId, PathMap};
use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};
use text_pos::Range;

/// The input to analysis.
#[derive(Debug)]
pub struct Input {
  /// A map from source paths to their contents.
  pub(crate) sources: PathMap<String>,
  /// A map from group paths to their (parsed) contents.
  pub(crate) groups: PathMap<mlb_hir::BasDec>,
  /// A map from group paths to their position databases.
  ///
  /// Invariant: keys(groups) == keys(groups_pos_dbs)
  pub(crate) groups_pos_dbs: PathMap<text_pos::PositionDb>,
  /// The root group id.
  pub(crate) root_group_id: PathId,
}

impl Input {
  /// Return an iterator over the source paths.
  pub fn iter_sources(&self) -> impl Iterator<Item = (PathId, &str)> + '_ {
    self.sources.iter().map(|(&path, s)| (path, s.as_str()))
  }
}

/// An error when getting input.
#[derive(Debug)]
pub struct GetInputError {
  source: Source,
  path: PathBuf,
  kind: GetInputErrorKind,
}

impl GetInputError {
  /// Returns a path associated with this error, which may or may not exist.
  pub fn path(&self) -> &Path {
    self.source.path.as_ref().unwrap_or(&self.path).as_path()
  }

  /// Returns a range for this error in `path`.
  pub fn range(&self) -> Option<Range> {
    self.source.range
  }

  /// Returns a value that displays the error message without the path.
  pub fn message(&self) -> impl fmt::Display + '_ {
    &self.kind
  }

  /// Returns the error code for this.
  pub fn to_code(&self) -> u8 {
    todo!()
  }
}

impl fmt::Display for GetInputError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}: {}", self.path.display(), self.kind)
  }
}

impl std::error::Error for GetInputError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match &self.kind {
      GetInputErrorKind::ReadDir(e)
      | GetInputErrorKind::ReadFile(e)
      | GetInputErrorKind::Canonicalize(e) => Some(e),
      GetInputErrorKind::Cm(e) => Some(e),
      GetInputErrorKind::Mlb(e) => Some(e),
      GetInputErrorKind::NotInRoot(e) => Some(e),
      GetInputErrorKind::CouldNotParseConfig(e) => Some(e),
      GetInputErrorKind::MultipleRoots(_, _)
      | GetInputErrorKind::NoRoot
      | GetInputErrorKind::InvalidConfigVersion(_)
      | GetInputErrorKind::Cycle
      | GetInputErrorKind::UnsupportedExport
      | GetInputErrorKind::NotGroup
      | GetInputErrorKind::Duplicate(_) => None,
    }
  }
}

#[derive(Debug)]
enum GetInputErrorKind {
  ReadDir(std::io::Error),
  ReadFile(std::io::Error),
  Canonicalize(std::io::Error),
  NotInRoot(std::path::StripPrefixError),
  MultipleRoots(PathBuf, PathBuf),
  NoRoot,
  NotGroup,
  CouldNotParseConfig(toml::de::Error),
  InvalidConfigVersion(u16),
  Cm(cm::Error),
  Mlb(mlb_syntax::Error),
  Cycle,
  UnsupportedExport,
  Duplicate(hir::Name),
}

impl fmt::Display for GetInputErrorKind {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      GetInputErrorKind::ReadDir(_) => write!(f, "couldn't read directory"),
      GetInputErrorKind::ReadFile(_) => write!(f, "couldn't read file"),
      GetInputErrorKind::Canonicalize(_) => write!(f, "couldn't canonicalize path"),
      GetInputErrorKind::NotInRoot(_) => write!(f, "not in root"),
      GetInputErrorKind::MultipleRoots(a, b) => write!(
        f,
        "multiple root groups: {} and {}",
        a.display(),
        b.display()
      ),
      GetInputErrorKind::NoRoot => f.write_str("no root group"),
      GetInputErrorKind::NotGroup => f.write_str("not a group path"),
      GetInputErrorKind::CouldNotParseConfig(_) => write!(f, "couldn't parse config"),
      GetInputErrorKind::InvalidConfigVersion(n) => {
        write!(f, "invalid config version: expected 1, found {n}")
      }
      GetInputErrorKind::Cm(_) => write!(f, "couldn't process SML/NJ CM file"),
      GetInputErrorKind::Mlb(_) => write!(f, "couldn't process ML Basis file"),
      GetInputErrorKind::Cycle => f.write_str("there is a cycle involving this path"),
      GetInputErrorKind::UnsupportedExport => f.write_str("unsupported export kind"),
      GetInputErrorKind::Duplicate(name) => write!(f, "duplicate name: {name}"),
    }
  }
}

/// std's Result with GetInputError as the default error.
pub type Result<T, E = GetInputError> = std::result::Result<T, E>;

/// A kind of group path.
#[derive(Debug, Clone, Copy)]
enum GroupPathKind {
  /// SML/NJ Compilation Manager files.
  Cm,
  /// ML Basis files.
  Mlb,
}

/// A group path.
#[derive(Debug)]
pub struct GroupPath {
  kind: GroupPathKind,
  path: PathBuf,
}

impl GroupPath {
  /// Returns a new `GroupPath`.
  pub fn new<F>(fs: &F, path: PathBuf) -> Option<GroupPath>
  where
    F: paths::FileSystem,
  {
    if !fs.is_file(path.as_path()) {
      return None;
    }
    let kind = match path.extension()?.to_str()? {
      "cm" => GroupPathKind::Cm,
      "mlb" => GroupPathKind::Mlb,
      _ => return None,
    };
    Some(GroupPath { path, kind })
  }

  /// Return this as a `Path`.
  pub fn as_path(&self) -> &Path {
    self.path.as_path()
  }
}

/// Get some input from the filesystem. If `root_group_path` is provided, it should be in the
/// `root`.
pub fn get<F>(
  fs: &F,
  root: &mut paths::Root,
  mut root_group_path: Option<GroupPath>,
) -> Result<Input>
where
  F: paths::FileSystem,
{
  let mut root_group_source = Source::default();
  // try to get from the config.
  if root_group_path.is_none() {
    let config_path = root.as_path().join(config::FILE_NAME);
    if let Ok(contents) = fs.read_to_string(&config_path) {
      let config: config::Root = match toml::from_str(&contents) {
        Ok(x) => x,
        Err(e) => {
          return Err(GetInputError {
            source: Source::default(),
            path: config_path,
            kind: GetInputErrorKind::CouldNotParseConfig(e),
          })
        }
      };
      if config.version != 1 {
        return Err(GetInputError {
          source: Source::default(),
          path: config_path,
          kind: GetInputErrorKind::InvalidConfigVersion(config.version),
        });
      }
      if let Some(path) = config.workspace.and_then(|workspace| workspace.root) {
        let path = root.as_path().join(path);
        match GroupPath::new(fs, path.clone()) {
          Some(path) => {
            root_group_source.path = Some(config_path);
            root_group_path = Some(path);
          }
          None => {
            return Err(GetInputError {
              source: Source {
                path: Some(config_path),
                range: None,
              },
              path,
              kind: GetInputErrorKind::NotGroup,
            })
          }
        }
      }
    }
  }
  // if not, try to get one from the root dir.
  if root_group_path.is_none() {
    let dir_entries = fs.read_dir(root.as_path()).map_err(|e| GetInputError {
      source: Source::default(),
      path: root.as_path().to_owned(),
      kind: GetInputErrorKind::ReadDir(e),
    })?;
    for entry in dir_entries {
      if let Some(group_path) = GroupPath::new(fs, entry.clone()) {
        match root_group_path {
          Some(rgp) => {
            return Err(GetInputError {
              kind: GetInputErrorKind::MultipleRoots(rgp.path.clone(), entry.clone()),
              source: Source {
                path: Some(rgp.path),
                range: None,
              },
              path: entry,
            })
          }
          None => root_group_path = Some(group_path),
        }
      }
    }
  }
  let root_group_path = root_group_path.ok_or_else(|| GetInputError {
    source: Source::default(),
    path: root.as_path().to_owned(),
    kind: GetInputErrorKind::NoRoot,
  })?;
  let root_group_id = get_path_id(fs, root, root_group_source, root_group_path.path.as_path())?;
  let mut sources = PathMap::<String>::default();
  let mut groups = PathMap::<mlb_hir::BasDec>::default();
  let mut groups_pos_dbs = PathMap::<text_pos::PositionDb>::default();
  let mut stack = vec![((root_group_id, None), root_group_id)];
  while let Some(((containing_path_id, containing_path_range), group_path_id)) = stack.pop() {
    if groups.contains_key(&group_path_id) {
      continue;
    }
    let group_path = root.get_path(group_path_id).clone();
    let group_path = group_path.as_path();
    let containing_path = root.get_path(containing_path_id).as_path().to_owned();
    let source = Source {
      path: Some(containing_path),
      range: containing_path_range,
    };
    let contents = read_file(fs, source, group_path)?;
    let pos_db = text_pos::PositionDb::new(&contents);
    let group_parent = group_path
      .parent()
      .expect("path from get_path has no parent");
    let group = match root_group_path.kind {
      GroupPathKind::Cm => {
        let cm = cm::get(&contents).map_err(|e| GetInputError {
          source: Source {
            path: None,
            range: pos_db.range(e.text_range()),
          },
          path: group_path.to_owned(),
          kind: GetInputErrorKind::Cm(e),
        })?;
        let paths = cm
          .paths
          .into_iter()
          .filter(|x| !path_is_dollar(x.val.as_path()))
          .map(|parsed_path| {
            let range = pos_db.range(parsed_path.range);
            let source = Source {
              path: Some(group_path.to_owned()),
              range,
            };
            let path = group_parent.join(parsed_path.val.as_path());
            let path_id = get_path_id(fs, root, source.clone(), path.as_path())?;
            let kind = match parsed_path.val.kind() {
              cm::PathKind::Sml => {
                let contents = read_file(fs, source, path.as_path())?;
                sources.insert(path_id, contents);
                mlb_hir::PathKind::Sml
              }
              cm::PathKind::Cm => {
                stack.push(((group_path_id, range), path_id));
                // NOTE this is a lie.
                mlb_hir::PathKind::Mlb
              }
            };
            Ok(mlb_hir::BasDec::Path(path_id, kind))
          })
          .collect::<Result<Vec<_>>>()?;
        let exports = cm
          .exports
          .into_iter()
          .map(|export| match export {
            cm::Export::Regular(ns, name) => {
              let ns = match ns.val {
                cm::Namespace::Structure => mlb_hir::Namespace::Structure,
                cm::Namespace::Signature => mlb_hir::Namespace::Signature,
                cm::Namespace::Functor => mlb_hir::Namespace::Functor,
                cm::Namespace::FunSig => {
                  return Err(GetInputError {
                    source: Source {
                      path: None,
                      range: pos_db.range(ns.range),
                    },
                    path: group_path.to_owned(),
                    kind: GetInputErrorKind::UnsupportedExport,
                  })
                }
              };
              Ok(mlb_hir::BasDec::Export(ns, name.clone(), name))
            }
            cm::Export::Library(lib) => Err(GetInputError {
              source: Source {
                path: None,
                range: pos_db.range(lib.range),
              },
              path: group_path.to_owned(),
              kind: GetInputErrorKind::UnsupportedExport,
            }),
          })
          .collect::<Result<Vec<_>>>()?;
        mlb_hir::BasDec::Local(
          mlb_hir::BasDec::seq(paths).into(),
          mlb_hir::BasDec::seq(exports).into(),
        )
      }
      GroupPathKind::Mlb => {
        let syntax_dec = mlb_syntax::get(&contents).map_err(|e| GetInputError {
          source: Source {
            path: None,
            range: pos_db.range(e.text_range()),
          },
          path: group_path.to_owned(),
          kind: GetInputErrorKind::Mlb(e),
        })?;
        let mut cx = LowerCx {
          path: group_path,
          parent: group_parent,
          pos_db: &pos_db,
          fs,
          root,
          sources: &mut sources,
          stack: &mut stack,
          path_id: group_path_id,
        };
        get_bas_dec(&mut cx, syntax_dec)?
      }
    };
    groups.insert(group_path_id, group);
    groups_pos_dbs.insert(group_path_id, pos_db);
  }
  let graph: topo_sort::Graph<_> = groups
    .iter()
    .map(|(&path, group)| {
      let mut ac = BTreeSet::<PathId>::new();
      bas_dec_paths(&mut ac, group);
      (path, ac)
    })
    .collect();
  if let Err(err) = topo_sort::get(&graph) {
    return Err(GetInputError {
      source: Source::default(),
      path: root.get_path(err.witness()).as_path().to_owned(),
      kind: GetInputErrorKind::Cycle,
    });
  }
  Ok(Input {
    sources,
    groups,
    groups_pos_dbs,
    root_group_id,
  })
}

/// NOTE: for now we just ignore dollar paths, since we include the full std basis
fn path_is_dollar(path: &Path) -> bool {
  path.as_os_str().to_string_lossy().contains('$')
}

#[derive(Debug, Default, Clone)]
struct Source {
  path: Option<PathBuf>,
  range: Option<Range>,
}

fn get_path_id<F>(
  fs: &F,
  root: &mut paths::Root,
  source: Source,
  path: &Path,
) -> Result<paths::PathId>
where
  F: paths::FileSystem,
{
  let canonical = fs.canonicalize(path).map_err(|e| GetInputError {
    source: source.clone(),
    path: path.to_owned(),
    kind: GetInputErrorKind::Canonicalize(e),
  })?;
  root.get_id(&canonical).map_err(|e| GetInputError {
    source,
    path: path.to_owned(),
    kind: GetInputErrorKind::NotInRoot(e),
  })
}

fn read_file<F>(fs: &F, source: Source, path: &Path) -> Result<String>
where
  F: paths::FileSystem,
{
  fs.read_to_string(path).map_err(|e| GetInputError {
    source,
    path: path.to_owned(),
    kind: GetInputErrorKind::ReadFile(e),
  })
}

struct LowerCx<'a, F> {
  path: &'a Path,
  parent: &'a Path,
  pos_db: &'a text_pos::PositionDb,
  fs: &'a F,
  root: &'a mut paths::Root,
  sources: &'a mut PathMap<String>,
  stack: &'a mut Vec<((PathId, Option<Range>), PathId)>,
  path_id: PathId,
}

fn get_bas_dec<F>(cx: &mut LowerCx<'_, F>, dec: mlb_syntax::BasDec) -> Result<mlb_hir::BasDec>
where
  F: paths::FileSystem,
{
  let ret = match dec {
    mlb_syntax::BasDec::Basis(binds) => {
      let mut names = FxHashSet::<hir::Name>::default();
      let binds = binds
        .into_iter()
        .map(|(name, exp)| {
          if !names.insert(name.val.clone()) {
            return Err(GetInputError {
              source: Source {
                path: None,
                range: cx.pos_db.range(name.range),
              },
              path: cx.path.to_owned(),
              kind: GetInputErrorKind::Duplicate(name.val),
            });
          }
          let exp = get_bas_exp(cx, exp)?;
          Ok(mlb_hir::BasDec::Basis(name, exp.into()))
        })
        .collect::<Result<Vec<_>>>()?;
      mlb_hir::BasDec::seq(binds)
    }
    mlb_syntax::BasDec::Open(names) => {
      mlb_hir::BasDec::seq(names.into_iter().map(mlb_hir::BasDec::Open).collect())
    }
    mlb_syntax::BasDec::Local(local_dec, in_dec) => mlb_hir::BasDec::Local(
      get_bas_dec(cx, *local_dec)?.into(),
      get_bas_dec(cx, *in_dec)?.into(),
    ),
    mlb_syntax::BasDec::Export(ns, binds) => {
      let mut names = FxHashSet::<hir::Name>::default();
      let binds = binds
        .into_iter()
        .map(|(lhs, rhs)| {
          if !names.insert(lhs.val.clone()) {
            return Err(GetInputError {
              source: Source {
                path: None,
                range: cx.pos_db.range(lhs.range),
              },
              path: cx.path.to_owned(),
              kind: GetInputErrorKind::Duplicate(lhs.val),
            });
          }
          let rhs = rhs.unwrap_or_else(|| lhs.clone());
          let ns = match ns {
            mlb_syntax::Namespace::Structure => mlb_hir::Namespace::Structure,
            mlb_syntax::Namespace::Signature => mlb_hir::Namespace::Signature,
            mlb_syntax::Namespace::Functor => mlb_hir::Namespace::Functor,
          };
          Ok(mlb_hir::BasDec::Export(ns, lhs, rhs))
        })
        .collect::<Result<Vec<_>>>()?;
      mlb_hir::BasDec::seq(binds)
    }
    mlb_syntax::BasDec::Path(parsed_path) => {
      if path_is_dollar(parsed_path.val.as_path()) {
        // HACK: use this as an empty dec instead of returning Result<Option<BasDec> and having this
        // be the only None case.
        return Ok(mlb_hir::BasDec::seq(vec![]));
      }
      let range = cx.pos_db.range(parsed_path.range);
      let source = Source {
        path: Some(cx.path.to_owned()),
        range,
      };
      let path = cx.parent.join(parsed_path.val.as_path());
      let path_id = get_path_id(cx.fs, cx.root, source.clone(), path.as_path())?;
      let kind = match parsed_path.val.kind() {
        mlb_syntax::PathKind::Sml => {
          let contents = read_file(cx.fs, source, path.as_path())?;
          cx.sources.insert(path_id, contents);
          mlb_hir::PathKind::Sml
        }
        mlb_syntax::PathKind::Mlb => {
          cx.stack.push(((cx.path_id, range), path_id));
          mlb_hir::PathKind::Mlb
        }
      };
      mlb_hir::BasDec::Path(path_id, kind)
    }
    mlb_syntax::BasDec::Ann(_, dec) => get_bas_dec(cx, *dec)?,
    mlb_syntax::BasDec::Seq(decs) => mlb_hir::BasDec::seq(
      decs
        .into_iter()
        .map(|dec| get_bas_dec(cx, dec))
        .collect::<Result<Vec<_>>>()?,
    ),
  };
  Ok(ret)
}

fn get_bas_exp<F>(cx: &mut LowerCx<'_, F>, exp: mlb_syntax::BasExp) -> Result<mlb_hir::BasExp>
where
  F: paths::FileSystem,
{
  let ret = match exp {
    mlb_syntax::BasExp::Bas(dec) => mlb_hir::BasExp::Bas(get_bas_dec(cx, dec)?),
    mlb_syntax::BasExp::Name(name) => mlb_hir::BasExp::Name(name),
    mlb_syntax::BasExp::Let(dec, exp) => {
      mlb_hir::BasExp::Let(get_bas_dec(cx, dec)?, get_bas_exp(cx, *exp)?.into())
    }
  };
  Ok(ret)
}

fn bas_dec_paths(ac: &mut BTreeSet<PathId>, dec: &mlb_hir::BasDec) {
  match dec {
    mlb_hir::BasDec::Open(_) | mlb_hir::BasDec::Export(_, _, _) => {}
    mlb_hir::BasDec::Path(p, _) => {
      ac.insert(*p);
    }
    mlb_hir::BasDec::Basis(_, exp) => bas_exp_paths(ac, exp),
    mlb_hir::BasDec::Local(local_dec, in_dec) => {
      bas_dec_paths(ac, local_dec);
      bas_dec_paths(ac, in_dec);
    }
    mlb_hir::BasDec::Seq(decs) => {
      for dec in decs {
        bas_dec_paths(ac, dec);
      }
    }
  }
}

fn bas_exp_paths(ac: &mut BTreeSet<PathId>, exp: &mlb_hir::BasExp) {
  match exp {
    mlb_hir::BasExp::Bas(dec) => bas_dec_paths(ac, dec),
    mlb_hir::BasExp::Name(_) => {}
    mlb_hir::BasExp::Let(dec, exp) => {
      bas_dec_paths(ac, dec);
      bas_exp_paths(ac, exp);
    }
  }
}
