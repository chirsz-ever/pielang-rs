use pielang::syntax::{GlobalStatemantParser, GrammerParser};
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
    let parser = GrammerParser::new();

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

        let mut buf = String::new();
        input.read_to_string(&mut buf).expect("read input failed");
        match parser.parse(&buf) {
            Ok(res) => {
                for e in res {
                    println!("{:?}", e);
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                return Ok(());
            }
        }
    }

    if should_repl(&opt) {
        repl();
    }

    Ok(())
}

// 有 `-i` 参数或无参数时开启 REPL
fn should_repl(opt: &Opt) -> bool {
    return opt.interactive || (opt.input.is_none() && opt.exprs.is_empty());
}

fn repl() {
    use rustyline::error::ReadlineError;
    use rustyline::{Cmd, Config, Editor};
    let conf = Config::builder().auto_add_history(true).build();
    let mut rl = Editor::<()>::with_config(conf);
    rl.bind_sequence(KeyEvent::ctrl('\\'), Cmd::Insert(1, String::from("λ")));
    let parser = GlobalStatemantParser::new();

    for readline in rl.iter("> ") {
        match readline {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                match parser.parse(&line) {
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
