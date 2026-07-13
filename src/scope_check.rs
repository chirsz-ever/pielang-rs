use crate::{core_ast, utils};
use core_ast::{Argument, Expr};
use fehler::throws;
use std::fmt;
use utils::{map_result, LocatedError, Ref, DBI};

pub type Env<'a> = utils::StackMap<Option<&'a str>, ()>;

pub type Error = LocatedError<ErrorKind>;

#[derive(Debug, Clone)]
pub enum ErrorKind {
    UndefinedIdentifier { ident: String },
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ErrorKind::*;
        match self {
            UndefinedIdentifier { ident } => {
                write!(f, "undefined identifier: `{}`", ident)
            }
        }
    }
}

/// 转换为 De Bruijn index 表示
#[throws]
pub fn to_dbi<M, 'a>(expr: &Expr<M, Ref<str>>, env: &Env<'a>) -> Expr<M, DBI> {
    use Expr::*;
    match expr {
        Info(_info, expr_inner) => to_dbi(expr_inner, env)?,
        NatLiteral(n) => NatLiteral(*n),
        AtomLiteral(s) => AtomLiteral(s.clone()),
        Identifier(ident) => Identifier(find_index(ident, env)?),
        LambdaExpr(arg, body) => {
            LambdaExpr(arg.clone(), Ref::new(to_dbi(body, &append_arg(env, arg))?))
        }
        PiExpr(arg, ty, body) => PiExpr(
            arg.clone(),
            Ref::new(to_dbi(ty, &append_arg(env, arg))?),
            Ref::new(to_dbi(body, &append_arg(env, arg))?),
        ),
        SigmaExpr(arg, ty, body) => SigmaExpr(
            arg.clone(),
            Ref::new(to_dbi(ty, &append_arg(env, arg))?),
            Ref::new(to_dbi(body, &append_arg(env, arg))?),
        ),
        Apply(f, arg) => Apply(Ref::new(to_dbi(f, env)?), Ref::new(to_dbi(arg, env)?)),
        BuiltinApply(bf, args) => {
            BuiltinApply(bf, map_result(args.iter(), |arg| to_dbi(arg, env))?)
        }
        BuiltinId(bid) => BuiltinId(bid),
    }
}

#[throws]
fn find_index<'a>(ident: &str, env: &Env<'a>) -> DBI {
    env.iter()
        .position(|(k, _)| *k == Some(ident))
        .ok_or_else(|| ErrorKind::UndefinedIdentifier {
            ident: ident.into(),
        })?
}

fn append_arg<'a>(env: &Env<'a>, arg: &'a Argument) -> Env<'a> {
    match arg {
        Argument::Dummy => env.insert(None, ()),
        Argument::Symbol(sym) => env.insert(Some(&**sym), ()),
    }
}

pub fn default_environment<'a>() -> Env<'a> {
    Env::new()
}
