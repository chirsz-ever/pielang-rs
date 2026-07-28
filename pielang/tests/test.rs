use std::fmt::Write;

use pielang::ast::{check_syntax, GlobalStatemant, Id};
use pielang::core::DBIPPrint as dpp;
use pielang::type_check as tc;

type Env = tc::Env;

fn interpret_to_string(source: &str) -> String {
    let mut output = String::new();
    let mut env = Env::new();
    let parser = pielang::syntax::GrammerParser::new();
    let stats = parser.parse(source).unwrap_or_else(|e| panic!("parse error: {}", e));

    for stmt in stats {
        match stmt {
            GlobalStatemant::Claim(_, Id(_, sym), ty) => {
                let env_1 = env.iter().map(|(k, _)| (k.as_deref(), ())).collect();
                check_syntax(&ty, &env_1).unwrap();
                let (_, ty_o) = tc::resolve_type(&ty, &env).unwrap();
                let ty_o = tc::normalize(&ty_o, &env);
                env = env.insert(Some(sym.into()), (ty_o, Default::default()));
            }
            GlobalStatemant::Define(_, Id(_, sym), expr) => {
                let env_1 = env.iter().map(|(k, _)| (k.as_deref(), ())).collect();
                check_syntax(&expr, &env_1).unwrap();
                let (ty, expr_ref) = env.get(&Some(sym.into())).unwrap();
                let e_o = tc::synthesize_with_type(&expr, ty, &env).unwrap();
                let e_o = tc::normalize(&e_o, &env);
                expr_ref.replace(Some(e_o));
            }
            GlobalStatemant::Expression(expr) => {
                let env_1 = env.iter().map(|(k, _)| (k.as_deref(), ())).collect();
                check_syntax(&expr, &env_1).unwrap();
                let (ty_s, e_s) = tc::synthesize(&expr, &env).unwrap();
                let ty_o = tc::normalize(&ty_s, &env);
                let e_o = tc::normalize(&e_s, &env);

                match ty_o {
                    pielang::core::Expr::S("U", _) => {
                        writeln!(output, "{}", dpp(&e_o, &env)).unwrap();
                    }
                    _ => {
                        writeln!(output, "(the {} {})", dpp(&ty_o, &env), dpp(&e_o, &env)).unwrap();
                    }
                }
            }
            GlobalStatemant::CheckSame(_, ty, e1, e2) => {
                let env_1 = env.iter().map(|(k, _)| (k.as_deref(), ())).collect();
                check_syntax(&e1, &env_1).unwrap();
                check_syntax(&e2, &env_1).unwrap();
                check_syntax(&ty, &env_1).unwrap();
                let (_, ty_o) = tc::resolve_type(&ty, &env).unwrap();
                let e1_o = tc::synthesize_with_type(&e1, &ty_o, &env).unwrap();
                let e2_o = tc::synthesize_with_type(&e2, &ty_o, &env).unwrap();
                let e1_o = tc::normalize(&e1_o, &env);
                let e2_o = tc::normalize(&e2_o, &env);
                let ty_o = tc::normalize(&ty_o, &env);
                tc::expr_check_same(&e1_o, &e2_o, &ty_o, &env).unwrap();
            }
        }
    }

    output
}

#[test]
fn test_pie_snapshots() {
    insta::glob!("snapshots/*.pie", |path| {
        let source = std::fs::read_to_string(path).unwrap();
        let output = interpret_to_string(&source);
        insta::assert_snapshot!(output);
    });
}
