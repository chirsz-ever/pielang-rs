//! 基本流程：  
//! 源代码 -> syntax::XXParser::parse -> core_ast::unfold -> type_check -> ...

// TODO: 处理 TODO
// TODO: 实现 check-same

use lalrpop_util::lalrpop_mod;

/// Stable-compatible stand-in for the never type `!`.
/// Used as a phantom type parameter where no meta-info is needed.
pub type Never = std::convert::Infallible;

lalrpop_mod!(#[allow(clippy::type_complexity)] pub syntax);

pub mod ast;
pub mod core;

#[cfg(test)]
mod test;
pub mod type_check;
pub mod utils;

pub use utils::Ref;
