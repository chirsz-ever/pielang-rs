use anyhow::Context as _;
use core_ast::DBIPPrint as dpp;
use fehler::throws;
use pielang::*;
use rustyline::KeyEvent;
use std::fs::File;
use std::io::{self, prelude::*};
use structopt::StructOpt;
use type_check as tc;

type Error = anyhow::Error;

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

#[throws]
fn main() {
    pretty_env_logger::init();
    let opt = Opt::from_args();
    let parser = syntax::ExprParser::new();

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

    for e in &opt.exprs {
        let expr = parser.parse(e).map_err(|err| anyhow::anyhow!("{}", err))?;
        let e_dbi = transform_expression(&expr)?;
        if opt.check_type_only {
            let (ty, e_o) = tc::synthesize(&e_dbi, &env)?;
            println!("(the {} {})", dpp(&ty, &env), dpp(&e_o, &env));
        } else {
            todo!("Implement evaluation of expressions from command line arguments");
        }
    }

    if should_repl(&opt) {
        repl(opt.check_type_only, &env)?;
    }
}

/// 从简单语法树到核心语法树
#[throws]
fn transform_expression(expr: &ast::Expr) -> core_ast::Expr<Never> {
    let unfold_expr = core_ast::unfold(expr)?;
    scope_check::to_dbi(&unfold_expr, &scope_check::default_environment())?
}

#[throws]
fn interpret_file(input: &mut dyn Read, _check_type_only: bool, _env: &mut Env) {
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
                let e = transform_expression(&expr)?;
                println!("{:?}", e);
            }
            CheckSame(_, _, _, _) => {
                todo!("Implement check-same in file interpretation");
            }
        }
    }
}

// 有 `-i` 参数或无参数时开启 REPL
fn should_repl(opt: &Opt) -> bool {
    opt.interactive || (opt.input.is_none() && opt.exprs.is_empty())
}

#[throws]
fn repl(check_type_only: bool, env: &Env) {
    use ast::GlobalStatemant::*;
    use rustyline::error::ReadlineError;
    use rustyline::{Cmd, Config, Editor};
    let conf = Config::builder().auto_add_history(true).build();
    let mut rl = Editor::<()>::with_config(conf);
    rl.bind_sequence(KeyEvent::ctrl('\\'), Cmd::Insert(1, String::from("λ")));
    let parser = syntax::GlobalStatemantParser::new();

    for readline in rl.iter("> ") {
        match readline {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                match parser.parse(&line).map_err(|e| anyhow::anyhow!("{}", e)) {
                    Ok(Expression(expr)) => {
                        let do_expr = || -> anyhow::Result<()> {
                            let e = transform_expression(&expr)?;
                            let (ty, e_o) =
                                tc::synthesize(&e, &env).context("Can't determine a type")?;
                            if check_type_only {
                                println!("(the {} {})", dpp(&ty, &env), dpp(&e_o, &env));
                            } else {
                                todo!("Implement evaluation of expressions in REPL");
                            }
                            Ok(())
                        };
                        match do_expr() {
                            Ok(()) => {}
                            Err(err) => eprintln!("Error: {:?}", err),
                        }
                    }
                    Ok(Define(_, _, _)) => {
                        eprintln!("`define` is not yet supported in REPL.")
                    }
                    Ok(Claim(_, _, _)) => {
                        eprintln!("`claim` is not yet supported in REPL.")
                    }
                    Ok(CheckSame(_, ty, e1, e2)) => {
                        let do_check_same = || -> anyhow::Result<()> {
                            let e1 = transform_expression(&e1)?;
                            let e2 = transform_expression(&e2)?;
                            let ty = transform_expression(&ty)?;
                            let (_, ty_o) = tc::resolve_type(&ty, env)?;
                            let e1_o = tc::synthesize_with_type(&e1, &ty_o, env)?;
                            let e2_o = tc::synthesize_with_type(&e2, &ty_o, env)?;
                            tc::expr_check_same(&e1_o, &e2_o, &ty, env)?;
                            Ok(())
                        };
                        match do_check_same() {
                            Ok(()) => {}
                            Err(err) => eprintln!("Error: {:?}", err),
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
}
