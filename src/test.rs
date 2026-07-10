use crate::type_check as tc;
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

#[test]
fn synthesize_tests() -> anyhow::Result<()> {
    let exprs = ["(the Nat 0)", "(the Atom 'a)", "(the Nat 'a)"];
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
