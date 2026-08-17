//! 基本流程：  
//! 源代码 -> syntax::XXParser::parse -> core_ast::unfold -> type_check -> ...

use lalrpop_util::lalrpop_mod;

lalrpop_mod!(#[allow(clippy::type_complexity)] pub syntax);

pub mod ast;
pub mod core;

pub mod type_check;
pub mod utils;
