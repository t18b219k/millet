//! Command-line arguments.

use gumdrop::Options;

pub fn get() -> Args {
  Args::parse_args_default_or_exit()
}

#[derive(Debug, Options)]
pub struct Args {
  #[options(free, help = "Source files")]
  pub files: Vec<String>,
  #[options(help = "Show this help")]
  pub help: bool,
  #[options(help = "Show the version")]
  pub version: bool,
  #[options(help = "Show AST")]
  pub show_ast: bool,
}
