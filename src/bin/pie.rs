use core_ast::DBIPPrint as dpp;
use pielang::*;
use rustyline::KeyEvent;
use std::fs::File;
use std::io::{self, prelude::*};
use structopt::StructOpt;
use type_check as tc;

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
    let parser = syntax::GlobalStatemantListParser::new();
    use ast::GlobalStatemant::*;
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
                    todo!(
                        "Only `expression` and `check-same` are supported in command line arguments"
                    )
                }
            }
        }
    }

    if should_repl(&opt) {
        repl(opt.check_type_only, &env)?;
    }
    Ok(())
}

fn process_expression(expr: &ast::Expr, env: &Env, check_type_only: bool) -> anyhow::Result<()> {
    let e_dbi = transform_expression(&expr)?;
    if check_type_only {
        let (ty, e_o) = tc::synthesize(&e_dbi, env)?;
        println!("(the {} {})", dpp(&ty, env), dpp(&e_o, env));
    } else {
        todo!("Implement evaluation of expressions");
    }
    Ok(())
}

fn process_check_same(
    ty: &ast::Expr,
    e1: &ast::Expr,
    e2: &ast::Expr,
    env: &Env,
) -> anyhow::Result<()> {
    let e1 = transform_expression(&e1)?;
    let e2 = transform_expression(&e2)?;
    let ty = transform_expression(&ty)?;
    let (_, ty_o) = tc::resolve_type(&ty, env)?;
    let e1_o = tc::synthesize_with_type(&e1, &ty_o, env)?;
    let e2_o = tc::synthesize_with_type(&e2, &ty_o, env)?;
    log::trace!("-----");
    tc::expr_check_same(&e1_o, &e2_o, &ty, env)?;
    Ok(())
}

/// 从简单语法树到核心语法树
fn transform_expression(expr: &ast::Expr) -> anyhow::Result<core_ast::Expr<Never>> {
    let unfold_expr = core_ast::unfold(expr)?;
    Ok(scope_check::to_dbi(&unfold_expr, &scope_check::default_environment())?)
}

fn interpret_file(input: &mut dyn Read, check_type_only: bool, env: &mut Env) -> anyhow::Result<()> {
    use ast::*;
    use GlobalStatemant::*;

    let parser = syntax::GrammerParser::new();
    let mut buf = String::new();
    input.read_to_string(&mut buf)?;
    let stats = parser
        .parse(&buf)
        .map_err(|err| anyhow::anyhow!("{}", err))?;
    for stmt in stats {
        match stmt {
            Claim(_, Symbol(_, sym), ty) => {
                let e = transform_expression(&ty)?;
                println!("Claim {}:", sym);
                println!("{:?}", e);
            }
            Define(_, Symbol(_, sym), expr) => {
                let e = transform_expression(&expr)?;
                println!("Define {} =", sym);
                println!("{:?}", e);
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

fn repl(check_type_only: bool, env: &Env) -> anyhow::Result<()> {
    use ast::GlobalStatemant::*;
    use rustyline::error::ReadlineError;
    use rustyline::history::MemHistory;
    use rustyline::{Cmd, Config, Editor};
    let conf = Config::builder().auto_add_history(true).build();
    let mut rl = Editor::<(), MemHistory>::with_history(conf, MemHistory::new())?;
    rl.bind_sequence(KeyEvent::ctrl('\\'), Cmd::Insert(1, String::from("λ")));
    let parser = syntax::GrammerParser::new();

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
                                Define(_, _, _) => {
                                    eprintln!("`define` is not yet supported in REPL.")
                                }
                                Claim(_, _, _) => {
                                    eprintln!("`claim` is not yet supported in REPL.")
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
