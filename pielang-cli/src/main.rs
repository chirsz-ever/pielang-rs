use anyhow::bail;
use pielang::ast::Ident;
use pielang::core_ast::DBIPPrint as dpp;
use pielang::type_check as tc;
use rustyline::KeyEvent;
use std::fs::File;
use std::io::{self, prelude::*};
use structopt::StructOpt;

type Env = tc::Env;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "pie-rs",
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
        let input: &mut dyn Read = if input_arg.as_os_str() == "-" {
            stdin_read = io::stdin();
            &mut stdin_read
        } else {
            file_read = File::open(&input_arg)?;
            &mut file_read
        };

        interpret_file(input, opt.check_type_only, &mut env)?;
    }

    // 处理 -e 参数
    let parser = pielang::syntax::GlobalStatemantListParser::new();
    use pielang::ast::GlobalStatemant::*;
    for e in &opt.exprs {
        let stats = parser.parse(e).map_err(|err| anyhow::anyhow!("{}", err))?;
        for stat in stats {
            match stat {
                Expression(expr) => {
                    process_expression(&expr, &env, opt.check_type_only)?;
                }
                CheckSame(_, ty, e1, e2) => {
                    process_check_same(&ty, &e1, &e2, &env)?;
                }
                _ => {
                    bail!(
                        "Only `expression` and `check-same` are supported in command line arguments"
                    );
                }
            }
        }
    }

    if should_repl(&opt) {
        repl(opt.check_type_only, &mut env)?;
    }
    Ok(())
}

fn process_expression(
    expr: &pielang::ast::Expr,
    env: &Env,
    check_type_only: bool,
) -> anyhow::Result<()> {
    let e_dbi = transform_expression(&expr, env)?;
    if check_type_only {
        let (ty, e_o) = tc::synthesize(&e_dbi, env)?;
        println!("(the {} {})", dpp(&ty, env), dpp(&e_o, env));
    } else {
        todo!("Implement evaluation of expressions");
    }
    Ok(())
}

fn process_check_same(
    ty: &pielang::ast::Expr,
    e1: &pielang::ast::Expr,
    e2: &pielang::ast::Expr,
    env: &Env,
) -> anyhow::Result<()> {
    let e1 = transform_expression(&e1, env)?;
    let e2 = transform_expression(&e2, env)?;
    let ty = transform_expression(&ty, env)?;
    let (_, ty_o) = tc::resolve_type(&ty, env)?;
    let e1_o = tc::synthesize_with_type(&e1, &ty_o, env)?;
    let e2_o = tc::synthesize_with_type(&e2, &ty_o, env)?;
    log::trace!("-----");
    tc::expr_check_same(&e1_o, &e2_o, &ty, env)?;
    Ok(())
}

fn process_claim(sym: &str, ty: &pielang::ast::Expr, env: &mut Env) -> anyhow::Result<()> {
    if env
        .iter()
        .any(|(k, _)| k.as_ref().is_some_and(|k| &**k == sym))
    {
        bail!("cannot reclaim `{}`", sym);
    }
    let ty = transform_expression(&ty, env)?;
    let (_, ty_o) = tc::resolve_type(&ty, env)?;
    *env = env.insert(Some(sym.into()), (ty_o, Default::default()));
    Ok(())
}

fn process_define(sym: &str, expr: &pielang::ast::Expr, env: &mut Env) -> anyhow::Result<()> {
    let Some((_, (ty, expr_ref))) = env
        .iter()
        .find(|(k, _)| k.as_ref().is_some_and(|k| &**k == sym))
    else {
        bail!("cannot define `{}` before claim", sym);
    };
    if expr_ref.borrow().is_some() {
        bail!("cannot redefine `{}`", sym);
    }
    let e_dbi = transform_expression(&expr, env)?;
    let e_o = tc::synthesize_with_type(&e_dbi, &ty, env)?;
    *expr_ref.borrow_mut() = Some(e_o);
    Ok(())
}

fn transform_expression(
    expr: &pielang::ast::Expr,
    env: &Env,
) -> anyhow::Result<pielang::core_ast::Expr<pielang::Never>> {
    let unfold_expr = pielang::core_ast::unfold(expr)?;
    let env_1 = env.iter().map(|(k, _)| (k.as_deref(), ())).collect();
    Ok(pielang::scope_check::to_dbi(&unfold_expr, &env_1)?)
}

fn interpret_file(
    input: &mut dyn Read,
    check_type_only: bool,
    env: &mut Env,
) -> anyhow::Result<()> {
    use pielang::ast::GlobalStatemant::*;

    let parser = pielang::syntax::GrammerParser::new();
    let mut buf = String::new();
    input.read_to_string(&mut buf)?;
    let stats = parser
        .parse(&buf)
        .map_err(|err| anyhow::anyhow!("{}", err))?;
    for stmt in stats {
        match stmt {
            Claim(_, Ident(_, sym), ty) => {
                process_claim(&sym, &ty, env)?;
            }
            Define(_, Ident(_, sym), expr) => {
                process_define(&sym, &expr, env)?;
            }
            Expression(expr) => {
                process_expression(&expr, env, check_type_only)?;
            }
            CheckSame(_, ty, e1, e2) => {
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
                match parser.parse(line).map_err(|e| anyhow::anyhow!("{}", e)) {
                    Ok(stats) => {
                        for stat in stats {
                            match stat {
                                Expression(expr) => {
                                    match process_expression(&expr, env, check_type_only) {
                                        Ok(()) => {}
                                        Err(err) => eprintln!("Error: {:?}", err),
                                    }
                                }
                                Define(_, Ident(_, sym), expr) => {
                                    process_define(&sym, &expr, env)
                                        .unwrap_or_else(|err| eprintln!("Error: {:?}", err));
                                }
                                Claim(_, Ident(_, sym), ty) => {
                                    process_claim(&sym, &ty, env)
                                        .unwrap_or_else(|err| eprintln!("Error: {:?}", err));
                                }
                                CheckSame(_, ty, e1, e2) => {
                                    match process_check_same(&ty, &e1, &e2, env) {
                                        Ok(()) => {}
                                        Err(err) => eprintln!("Error: {:?}", err),
                                    }
                                }
                            }
                        }
                    }
                    Err(err) => {
                        println!("Error: {}", err);
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
