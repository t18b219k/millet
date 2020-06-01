use std::fmt;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Ident {
  inner: String,
}

impl Ident {
  pub fn new(inner: String) -> Self {
    Self { inner }
  }
}

impl fmt::Display for Ident {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.inner.fmt(f)
  }
}
