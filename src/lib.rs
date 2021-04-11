//! 基本流程：  
//! 源代码 -> syntax::XXParser::parse -> core_ast::unfold -> type_check -> ...

// TODO: 处理 TODO
// TODO: 实现 check-same

#![feature(never_type)]

use lalrpop_util::lalrpop_mod;

lalrpop_mod!(#[allow(clippy::all)] pub syntax);

pub mod ast;
pub mod core_ast;
#[cfg(test)]
mod test;
pub mod type_check;
pub mod utils;

pub use utils::Ref;
