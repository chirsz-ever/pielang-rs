use lalrpop_util::lalrpop_mod;

lalrpop_mod!(#[allow(clippy::all)] pub syntax);

mod ast;
#[cfg(test)]
mod test;
mod utils;

pub use utils::Ref;
