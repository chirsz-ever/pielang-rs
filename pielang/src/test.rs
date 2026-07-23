use crate::ast::check_syntax;
use crate::core;
use crate::utils::StackMap;
use crate::{ast, type_check as tc};
use core::DBIPPrint as dpp;

#[test]
fn check_name() {
    let parser = crate::syntax::GlobalStatemantParser::new();
    let stats = [
        "(claim x Nat)",
        "(claim x)",
        "(claim x y z)",
        "(claim claim Nat)",
        "(claim U Nat)",
        "(define x 0)",
        "(define x)",
        "(define x y z)",
        "(define define 0)",
        "(define check-same 0)",
        "(define f (λ (U) 0))",
        "(define f (λ (sole) 0))",
        "(define f (λ (Pair) 0))",
        "(define f (λ (claim) 0))",
        "(define f (λ (define) 0))",
        "(define f (Pi ((U Nat)) Atom))",
        "(define f (Pi ((x Nat)(U Nat)) Atom))",
        "(define f (Sigma ((U Nat)) Atom))",
        "(define f (Sigma ((x Nat)(U Nat)) Atom))",
        "(check-same Nat 0 0)",
        "(check-same a)",
        "(check-same a b)",
        "(check-same a b c d)",
    ];
    for e in stats {
        insta::with_settings!({
            description => e,
        }, {
            let result;
            match parser.parse(e) {
                Ok(_) => {
                    result = "OK".to_string();
                }
                Err(err) => {
                    result = format!("Error: {}", err);
                }
            }
            insta::assert_debug_snapshot!(format!("check_name_{}", e), result);
        });
    }
}

#[test]
fn parse_expression() {
    let parser = crate::syntax::ExprParser::new();
    let exprs = [
        // Nat literals
        "0",
        "1",
        "9876",
        "01",
        // FIXME: Pie 拒绝了 -1
        // "-1",

        // Atom literals
        "'a",
        "'-a",
        "'a-",
        "'atom",
        "'this-is-a-symbol",
        "'  btom",
        "(quote ctom)",
        "(quote this-is-a-symbol)",
        "(quote 'a)",
        // symbols
        "nil",
        // S-expressions
        "(the (List Nat) nil)",
        "(the(List Nat)nil)",
        "(cons 2 (same 2))",
        "(lambda (x) x)",
        "(λ (x) (add1 x))",
        r"(the (Σ ((n Nat))
         (= Nat n n))
    (cons 2 (same 2)))",
        // brackets and braces
        "[the Nat 1]",
        "{the Nat 1}",
    ];
    for e in exprs {
        insta::with_settings!({
            description => e,
        }, {
            insta::assert_debug_snapshot!(format!("parse_expression_{}", e), parser.parse(e),);
        });
    }
}

fn check_synthesize(expr: &str) -> anyhow::Result<String> {
    let parser = crate::syntax::ExprParser::new();
    let expr = parser
        .parse(expr)
        .map_err(|err| anyhow::anyhow!("{}", err))?;

    let env = tc::Env::new();
    check_expression(&expr)?;
    let (ty_o, e_o) = tc::synthesize(&expr, &env)?;
    Ok(format!(
        "type: {}\nexpr: {}\n",
        dpp(&ty_o, &env),
        dpp(&e_o, &env)
    ))
}

fn check_expression(expr: &ast::Expr) -> anyhow::Result<()> {
    check_syntax(expr, &StackMap::new())?;
    Ok(())
}

fn check_same(ty: ast::Expr, e1: ast::Expr, e2: ast::Expr) -> anyhow::Result<()> {
    check_expression(&e1)?;
    check_expression(&e2)?;
    check_expression(&ty)?;
    let env = tc::Env::new();
    let (_, ty_o) = tc::resolve_type(&ty, &env)?;
    let e1_o = tc::synthesize_with_type(&e1, &ty_o, &env)?;
    let e2_o = tc::synthesize_with_type(&e2, &ty_o, &env)?;
    tc::expr_check_same(&e1_o, &e2_o, &ty_o, &env)?;
    Ok(())
}

#[test]
fn synthesize_tests() {
    let exprs = [
        // Nat
        "(the U Nat)",
        "zero",
        "(add1 zero)",
        "114",
        "(the Nat 0)",
        "(the Nat zero)",
        "(the Nat (add1 zero))",
        "(the Nat 114)",
        // Atom
        "(the U Atom)",
        "'a",
        "(quote atom)",
        "(the Atom 'a)",
        // Trivial
        "(the U Trivial)",
        "sole",
        "(the Trivial sole)",
        // Absurd
        "(the U Absurd)",
        "(the (→ Absurd Nat) (λ (nope) (ind-Absurd nope Nat)))",
        "(the (→ Absurd Nat) (λ (nope) (ind-Absurd (the Absurd nope) Nat)))",
        // lambda
        "(the (→ Nat Nat) (λ (x) x))",
        "(the (→ Nat Nat Nat) (λ (x y) x))",
        "(the (→ Nat Nat) (λ (x) (add1 x)))",
        "(the (-> Nat Nat) (lambda (x) ((the (-> Atom Nat) (lambda (y) 0)) 'a)))",
        "(the (-> Nat Nat) (lambda (x) ((the (-> Atom Nat) (lambda (y) (add1 x))) 'a)))",
        "(the (-> (-> (-> Nat Nat) Nat Nat) Nat Nat) (lambda (f x) (f (lambda (y) y) x)))",
        // Error cases
        "(the Nat 'a)",
        "(the Atom zero)",
        "(the Trivial 0)",
        "(the Trivial 'a)",
        "(the Absurd 0)",
        "(the 0 'a)",
        "(the Nat U)",
        "(the U 'a)",
        // scope error
        "x",
        "add1",
        "(zero 0)",
        "(λ (Nat) 0)",
        "(λ (x) y)",
        "(λ (x) add1)",
        "(Π ((zero Nat)) Nat)",
        "(Σ ((zero Nat)) Nat)",
    ];
    for s in exprs {
        eprintln!("{} ... ", s);
        let output = check_synthesize(s).unwrap_or_else(|e| format!("Error: {e}"));
        insta::with_settings!({
            description => s,
        }, {
            insta::assert_snapshot!(format!("check_synthesize_{}", s), output);
        });
    }
}

fn check_statement(expr: &str) -> anyhow::Result<String> {
    use crate::syntax::GlobalStatemantParser;
    let parser = GlobalStatemantParser::new();
    let expr = parser
        .parse(expr)
        .map_err(|err| anyhow::anyhow!("{}", err))?;
    match expr {
        ast::GlobalStatemant::Expression(e) => {
            check_expression(&e)?;
            let env = tc::Env::new();
            let (ty_o, e_o) = tc::synthesize(&e, &env)?;
            Ok(format!(
                "type: {}\nexpr: {}\n",
                dpp(&ty_o, &env),
                dpp(&e_o, &env)
            ))
        }
        ast::GlobalStatemant::CheckSame(_, ty, e1, e2) => {
            check_same(ty, e1, e2)?;
            Ok("OK".to_string())
        }
        _ => todo!("Other statements are not supported yet"),
    }
}

#[test]
fn tlt_tests() {
    let exprs = [
        "(the U (Pair Atom Atom))",
        "(the (Pair Atom Atom) (cons 'ratatouille 'baguette))",
        "(the (Pair Atom Nat) (cons 'ratatouille 0))",
        "(the (Pair Atom Atom) (cons 'ratatouille 0))",
        "(check-same (Pair Atom Atom) (cons 'aubergine 'courgette) (cons 'aubergine 'courgette))",
        "(check-same (Pair Atom Atom) (cons 'aubergine 'courgette) (cons 'aubergine 'bbb))",
        "(check-same U Atom Atom)",
        "(check-same U Atom Nat)",
        "(check-same U (Pair Atom Nat) (Pair Atom Nat))",
        "(check-same U (Pair Nat Atom) (Pair Atom Nat))",
        "(check-same Nat 0 0)",
        "(check-same Nat 0 1)",
        "(check-same Nat zero 0)",
        "(check-same Nat zero (add1 zero))",
        "(check-same Nat 1 (add1 zero))",
        "(check-same Nat (add1 zero) (add1 zero))",
        "(check-same (→ Nat Nat) (λ (x) x) (λ (x) x))",
        "(check-same (→ Nat Nat) (λ (x) x) (λ (y) y))",
        "(check-same (→ Nat Nat) (λ (x) x) (λ (y) 0))",
        "(check-same (→ Nat (Pair Nat Nat)) (λ (a) (cons a a)) (λ (d) (cons d d)))",
        "(check-same (→ Atom Nat Atom) (λ (x y) x) (λ (a b) a))",
    ];
    for s in exprs {
        eprintln!("{} ... ", s);
        insta::with_settings!({
            description => s,
        }, {
            let output = check_statement(s).unwrap_or_else(|e| format!("Error: {e}"));
            insta::assert_snapshot!(format!("tll_1_{}", s), output);
        });
    }
}
