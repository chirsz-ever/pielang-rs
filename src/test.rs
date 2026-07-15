use crate::ast::GlobalStatemant::CheckSame;
use crate::{ast, type_check as tc, Never};
use crate::{core_ast, scope_check};
use core_ast::DBIPPrint as dpp;

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
    let unfold_expr = core_ast::unfold(&expr)?;
    let e_dbi = scope_check::to_dbi(&unfold_expr, &scope_check::default_environment())?;
    let env = tc::Env::new();
    let mut output = String::new();
    match tc::synthesize(&e_dbi, &env) {
        Ok((ty, e_o)) => {
            output += &format!("type: {}\n", dpp(&ty, &env));
            output += &format!("expr: {}\n", dpp(&e_o, &env));
        }
        Err(err) => {
            output += &format!("error: {}", err);
        }
    }
    Ok(output)
}

fn transform_expression(expr: &ast::Expr) -> Result<core_ast::Expr<Never>, anyhow::Error> {
    let unfold_expr = core_ast::unfold(expr)?;
    let dbi = scope_check::to_dbi(&unfold_expr, &scope_check::default_environment())?;
    Ok(dbi)
}

fn check_same(expr: &str) -> anyhow::Result<String> {
    use crate::syntax::GlobalStatemantParser;
    let parser = GlobalStatemantParser::new();
    let expr = parser
        .parse(expr)
        .map_err(|err| anyhow::anyhow!("{}", err))?;
    let CheckSame(_, ty, e1, e2) = expr else {
        anyhow::bail!("Expected check-same statement");
    };
    let e1 = transform_expression(&e1)?;
    let e2 = transform_expression(&e2)?;
    let ty = transform_expression(&ty)?;
    let env = tc::Env::new();
    let (_, ty_o) = tc::resolve_type(&ty, &env)?;
    let e1_o = tc::synthesize_with_type(&e1, &ty_o, &env)?;
    let e2_o = tc::synthesize_with_type(&e2, &ty_o, &env)?;
    let mut output = String::new();
    match tc::expr_check_same(&e1_o, &e2_o, &ty, &env) {
        Ok(_) => output += "OK",
        Err(err) => output += &format!("error: {}", err),
    }
    Ok(output)
}

#[test]
fn synthesize_tests() -> anyhow::Result<()> {
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
        "(the Nat 'a)",
        // Atom
        "(the U Atom)",
        "'a",
        "(quote atom)",
        "(the Atom 'a)",
        "(the Atom zero)",
        // Trivial
        "(the U Trivial)",
        "sole",
        "(the Trivial sole)",
        "(the Trivial 0)",
        "(the Trivial 'a)",
        // Absurd
        "(the U Absurd)",
        "(the Absurd 0)",
        "(the (→ Absurd Nat) (λ (nope) (ind-Absurd nope Nat)))",
        "(the (→ Absurd Nat) (λ (nope) (ind-Absurd (the Absurd nope) Nat)))",
    ];
    for s in exprs {
        let output = check_synthesize(s)?;
        insta::with_settings!({
            description => s,
        }, {
            insta::assert_snapshot!(format!("check_synthesize_{}", s), output);
        });
    }
    Ok(())
}

#[test]
fn tlt_tests() -> anyhow::Result<()> {
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
    ];
    for s in exprs {
        let output;
        eprintln!("{} ... ", s);
        if s.starts_with("(the") {
            output = check_synthesize(s)?;
        } else if s.starts_with("(check-same") {
            output = check_same(s)?;
        } else {
            todo!();
        }
        insta::with_settings!({
            description => s,
        }, {
            insta::assert_snapshot!(format!("tll_1_{}", s), output);
        });
    }
    Ok(())
}
