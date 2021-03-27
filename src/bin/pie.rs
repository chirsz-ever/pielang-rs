use pielang::*;
use rustyline::KeyEvent;
use std::fs::File;
use std::io::{self, prelude::*};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "pie-rs",
    about = "Pie language interpreter implemented by Rust"
)]
struct Opt {
    /// Input file, use `-` to read from stdin.
    #[structopt(name = "FILE", parse(from_os_str))]
    pub input: Option<std::path::PathBuf>,
    /// Open REPL
    #[structopt(short, long = "repl")]
    pub interactive: bool,
    /// Read and eval a pie expression from command line arguments
    #[structopt(short, long = "eval")]
    pub exprs: Vec<String>,
}

fn main() -> io::Result<()> {
    let opt = Opt::from_args();

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

        match interpret(input) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }
    } else if should_repl(&opt) {
        repl();
    }

    Ok(())
}

fn analyze_expression(expr: &ast::Expr) -> anyhow::Result<core_ast::Expr<()>> {
    let e = core_ast::unfold(expr);
    core_ast::check_builtin(&e).map_err(|err| anyhow::anyhow!("{}", err))?;
    Ok(e)
}

fn interpret(input: &mut dyn Read) -> anyhow::Result<()> {
    use ast::*;
    use GlobalStatemant::*;

    let parser = syntax::GrammerParser::new();
    let mut buf = String::new();
    input.read_to_string(&mut buf).expect("read input failed");
    let stats = parser
        .parse(&buf)
        .map_err(|err| anyhow::anyhow!("{}", err))?;
    for stat in stats {
        match stat {
            Claim(_, Symbol(_, sym), ty) => {
                let e = analyze_expression(&ty)?;
                println!("Claim {}:", sym);
                println!("{:?}", e);
            }
            Define(_, Symbol(_, sym), expr) => {
                let e = analyze_expression(&expr)?;
                println!("Define {} =", sym);
                println!("{:?}", e);
            }
            Expression(expr) => {
                let e = analyze_expression(&expr)?;
                println!("{:?}", e);
            }
        }
    }
    Ok(())
}

// 有 `-i` 参数或无参数时开启 REPL
fn should_repl(opt: &Opt) -> bool {
    opt.interactive || (opt.input.is_none() && opt.exprs.is_empty())
}

fn repl() {
    use rustyline::error::ReadlineError;
    use rustyline::{Cmd, Config, Editor};
    let conf = Config::builder().auto_add_history(true).build();
    let mut rl = Editor::<()>::with_config(conf);
    rl.bind_sequence(KeyEvent::ctrl('\\'), Cmd::Insert(1, String::from("λ")));
    let parser = syntax::ExprParser::new();

    for readline in rl.iter("> ") {
        match readline {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                match parser
                    .parse(&line)
                    .map_err(|e| anyhow::anyhow!("{}", e))
                    .and_then(|expr| analyze_expression(&expr))
                {
                    Ok(ret) => {
                        println!("{:?}", ret);
                    }
                    Err(e) => {
                        println!("Error: {}", e);
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
