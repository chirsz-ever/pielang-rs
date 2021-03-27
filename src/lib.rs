//! 基本流程：  
//! 源代码 -> syntax::XXParser::parse -> core_ast::unfold -> core_ast::check_builtin -> ...

use lalrpop_util::lalrpop_mod;

lalrpop_mod!(#[allow(clippy::all)] pub syntax);

mod ast;
mod core_ast;
#[cfg(test)]
mod test;
mod utils;

pub use utils::Ref;
