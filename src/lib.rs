use lalrpop_util::lalrpop_mod;

lalrpop_mod!(pub syntax);

mod ast;
#[cfg(test)]
mod test;
