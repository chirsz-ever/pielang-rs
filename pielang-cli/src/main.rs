use anyhow::bail;
use pielang::ast::{Id, check_syntax};
use pielang::core::DBIPPrint as dpp;
use pielang::core::Expr::Nat;
use pielang::type_check as tc;
use pielang::utils::{ErrorKind, LocatedError, Span};
use rustyline::KeyEvent;
use std::fs::File;
use std::io::{self, prelude::*};
use structopt::StructOpt;

type Env = tc::Env;

/// 尝试从 anyhow::Error 中提取 LocatedError 的位置与不含 Span 的消息文本。
fn locate_error(err: &anyhow::Error) -> Option<(Option<Span>, String)> {
    if let Some(e) = err.downcast_ref::<LocatedError<ErrorKind>>() {
        return Some((e.loc, format!("{}", e.erk)));
    }
    None
}

/// 将源代码的字节偏移转换为 1-based 的行列号。
fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            return (line, col);
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// 若错误是 LocatedError，则在前缀追加文件名与行列信息。
fn format_error(err: &anyhow::Error, source: &str, filename: &str) -> String {
    if let Some((loc, msg)) = locate_error(err) {
        if let Some(Span(l, r)) = loc {
            let (sl, sc) = offset_to_line_col(source, l);
            let (el, ec) = offset_to_line_col(source, r);
            if sl == el {
                return format!("{}:{}:{}-{}: {}", filename, sl, sc, ec, msg);
            }
            return format!("{}:{}:{}-{}:{}: {}", filename, sl, sc, el, ec, msg);
        }
        return format!("{}: {}", filename, msg);
    }
    format!("{}: {}", filename, err)
}

/// 在给定源码上下文中执行 `f`，如返回错误则统一打印为 `Error: ...`，并返回是否失败。
fn run_with_source<F>(source: &str, filename: &str, f: F) -> bool
where
    F: FnOnce() -> anyhow::Result<()>,
{
    match f() {
        Ok(()) => false,
        Err(err) => {
            eprintln!("Error: {}", format_error(&err, source, filename));
            true
        }
    }
}

/// 将 lalrpop 的 ParseError 转换为 anyhow::Error，尽量保留 LocatedError 的位置信息。
fn parse_error_to_anyhow<T, E>(
    err: lalrpop_util::ParseError<usize, T, LocatedError<E>>,
) -> anyhow::Error
where
    T: std::fmt::Display,
    E: std::fmt::Debug + std::fmt::Display + Send + Sync + 'static,
{
    use lalrpop_util::ParseError::*;
    match err {
        User { error } => anyhow::Error::new(error),
        InvalidToken { location } => anyhow::Error::new(LocatedError {
            loc: Some(Span(location, location)),
            erk: "invalid token".to_string(),
        }),
        UnrecognizedEof { location, expected } => anyhow::Error::new(LocatedError {
            loc: Some(Span(location, location)),
            erk: format!(
                "unexpected end of file, expected one of: {}",
                expected.join(", ")
            ),
        }),
        UnrecognizedToken {
            token: (l, t, r),
            expected,
        } => anyhow::Error::new(LocatedError {
            loc: Some(Span(l, r)),
            erk: format!(
                "unrecognized token `{}`, expected one of: {}",
                t,
                expected.join(", ")
            ),
        }),
        ExtraToken { token: (l, t, r) } => anyhow::Error::new(LocatedError {
            loc: Some(Span(l, r)),
            erk: format!("extra token `{}`", t),
        }),
    }
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "pielang-rs",
    about = "Pie language interpreter implemented with Rust"
)]
struct Opt {
    /// Input file, use `-` to read from stdin.
    #[structopt(name = "FILE", parse(from_os_str))]
    pub input: Option<std::path::PathBuf>,
    /// Open REPL
    #[structopt(short, long = "repl")]
    pub interactive: bool,
    /// Only run check type
    #[structopt(short, long = "check")]
    pub check_type_only: bool,
    /// Read and eval a pie expression from command line arguments
    #[structopt(short, long = "eval")]
    pub exprs: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();
    let opt = Opt::from_args();

    let mut env = Env::new();

    // 如果有文件参数，先解释文件
    if let Some(input_arg) = opt.input.as_ref() {
        let (mut stdin_read, mut file_read);
        let filename: String;
        let input: &mut dyn Read = if input_arg.as_os_str() == "-" {
            stdin_read = io::stdin();
            filename = "stdin".to_string();
            &mut stdin_read
        } else {
            file_read = File::open(input_arg)?;
            filename = input_arg.display().to_string();
            &mut file_read
        };

        let mut buf = String::new();
        input.read_to_string(&mut buf)?;
        if run_with_source(&buf, &filename, || {
            interpret_file(&buf, opt.check_type_only, &mut env)
        }) {
            std::process::exit(1);
        }
    }

    // 处理 -e 参数
    for e in &opt.exprs {
        if run_with_source(e, "-e", || eval_arg(e, opt.check_type_only, &mut env)) {
            std::process::exit(1);
        }
    }

    if should_repl(&opt) {
        repl(opt.check_type_only, &mut env)?;
    }
    Ok(())
}

fn eval_arg(source: &str, check_type_only: bool, env: &mut Env) -> anyhow::Result<()> {
    use pielang::ast::GlobalStatemant::*;
    let parser = pielang::syntax::GlobalStatemantListParser::new();
    let stats = parser.parse(source).map_err(parse_error_to_anyhow)?;
    for stat in stats {
        match stat {
            Expression(expr) => process_expression(&expr, env, check_type_only)?,
            CheckSame(_, ty, e1, e2) => process_check_same(&ty, &e1, &e2, env)?,
            _ => {
                bail!("Only `expression` and `check-same` are supported in command line arguments")
            }
        }
    }
    Ok(())
}

fn process_expression(
    expr: &pielang::ast::Expr,
    env: &Env,
    check_type_only: bool,
) -> anyhow::Result<()> {
    check_expression(expr, env)?;
    let (ty_s, e_s) = tc::synthesize(expr, env)?;
    let (ty_o, e_o);
    if check_type_only {
        ty_o = ty_s;
        e_o = e_s;
    } else {
        ty_o = tc::normalize(&ty_s, env);
        e_o = tc::normalize(&e_s, env);
    }

    match &ty_o {
        pielang::core::Expr::S("U", args) if !matches!(args.as_slice(), [Nat(0)]) => {
            // > When an expression is a type, but does not have a type, Pie replies with just its normal form.
            // > -- Recess - Forkful of Pie
            println!("{}", dpp(&e_o, env));
        }
        _ => {
            println!("(the {} {})", dpp(&ty_o, env), dpp(&e_o, env));
        }
    }
    Ok(())
}

fn process_check_same(
    ty: &pielang::ast::Expr,
    e1: &pielang::ast::Expr,
    e2: &pielang::ast::Expr,
    env: &Env,
) -> anyhow::Result<()> {
    check_expression(e1, env)?;
    check_expression(e2, env)?;
    check_expression(ty, env)?;
    let (_, ty_o) = tc::resolve_type(ty, env)?;
    let e1_o = tc::synthesize_with_type(e1, &ty_o, env)?;
    let e2_o = tc::synthesize_with_type(e2, &ty_o, env)?;

    let e1_o = tc::normalize(&e1_o, env);
    let e2_o = tc::normalize(&e2_o, env);
    let ty_o = tc::normalize(&ty_o, env);

    log::trace!("-----");
    tc::expr_check_same(&e1_o, &e2_o, &ty_o, env)?;
    Ok(())
}

fn process_claim(sym: &str, ty: &pielang::ast::Expr, env: &mut Env) -> anyhow::Result<()> {
    if env
        .iter()
        .any(|(k, _)| k.as_ref().is_some_and(|k| &**k == sym))
    {
        bail!("cannot reclaim `{}`", sym);
    }
    check_expression(ty, env)?;
    let (_, ty_o) = tc::resolve_type(ty, env)?;
    let ty_o = tc::normalize(&ty_o, env);
    *env = env.insert(Some(sym.into()), (ty_o, Default::default()));
    Ok(())
}

fn process_define(sym: &str, expr: &pielang::ast::Expr, env: &mut Env) -> anyhow::Result<()> {
    let Some((ty, expr_ref)) = env.get(&Some(sym.into())) else {
        bail!("cannot define `{}` before claim", sym);
    };
    if expr_ref.borrow().is_some() {
        bail!("cannot redefine `{}`", sym);
    }
    check_expression(expr, env)?;
    let e_o = tc::synthesize_with_type(expr, ty, env)?;
    let e_o = tc::normalize(&e_o, env);
    expr_ref.replace(Some(e_o));
    Ok(())
}

fn check_expression(expr: &pielang::ast::Expr, env: &Env) -> anyhow::Result<()> {
    let env_1 = env.iter().map(|(k, _)| (k.as_deref(), ())).collect();
    check_syntax(expr, &env_1)?;
    Ok(())
}

fn interpret_file(source: &str, check_type_only: bool, env: &mut Env) -> anyhow::Result<()> {
    use pielang::ast::GlobalStatemant::*;

    let parser = pielang::syntax::GrammerParser::new();
    let stats = parser.parse(source).map_err(parse_error_to_anyhow)?;
    for stmt in stats {
        match stmt {
            Claim(_, Id(_, sym), ty) => {
                process_claim(sym, &ty, env)?;
            }
            Define(_, Id(_, sym), expr) => {
                process_define(sym, &expr, env)?;
            }
            Expression(expr) => {
                process_expression(&expr, env, check_type_only)?;
            }
            CheckSame(_, ty, e1, e2) => {
                // TODO: attach location information
                process_check_same(&ty, &e1, &e2, env)?;
            }
        }
    }
    Ok(())
}

// 有 `-i` 参数或无参数时开启 REPL
fn should_repl(opt: &Opt) -> bool {
    opt.interactive || (opt.input.is_none() && opt.exprs.is_empty())
}

fn repl(check_type_only: bool, env: &mut Env) -> anyhow::Result<()> {
    use pielang::ast::GlobalStatemant::*;
    use rustyline::error::ReadlineError;
    use rustyline::history::MemHistory;
    use rustyline::{Cmd, Config, Editor};
    let conf = Config::builder().auto_add_history(true).build();
    let mut rl = Editor::<(), MemHistory>::with_history(conf, MemHistory::new())?;
    rl.bind_sequence(KeyEvent::ctrl('\\'), Cmd::Insert(1, String::from("λ")));
    let parser = pielang::syntax::GrammerParser::new();

    for readline in rl.iter("> ") {
        match readline {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                match parser.parse(line) {
                    Ok(stats) => {
                        for stat in stats {
                            run_with_source(line, "REPL", || match stat {
                                Expression(expr) => process_expression(&expr, env, check_type_only),
                                Define(_, Id(_, sym), expr) => process_define(sym, &expr, env),
                                Claim(_, Id(_, sym), ty) => process_claim(sym, &ty, env),
                                CheckSame(_, ty, e1, e2) => process_check_same(&ty, &e1, &e2, env),
                            });
                        }
                    }
                    Err(err) => {
                        run_with_source(line, "REPL", || Err(parse_error_to_anyhow(err)));
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("Exit");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("Exit");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    Ok(())
}
