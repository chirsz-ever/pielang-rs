use crate::{core_ast, scope_check};
use core_ast::DBIPPrint as dpp;
use crate::type_check as tc;

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

#[test]
fn check_synthesize() -> anyhow::Result<()> {
    let parser = crate::syntax::ExprParser::new();
    let exprs = ["(the Nat 0)", "(the Atom 'a)", "(the Nat 'a)"];
    for s in exprs {
        insta::with_settings!({
            description => s,
        }, {
            let expr = parser.parse(s).unwrap();
            let unfold_expr = core_ast::unfold(&expr).unwrap();
            let e_dbi = scope_check::to_dbi(&unfold_expr, &scope_check::default_environment()).unwrap();
            let env = tc::Env::new();
            match tc::synthesize(&e_dbi, &env) {
                Ok((ty, e_o)) => {
                    insta::assert_snapshot!(format!("check_synthesize_{}", s), format!("type: {}\nexpr: {}\n", dpp(&ty, &env), dpp(&e_o, &env)));
                }
                Err(err) => {
                    insta::assert_snapshot!(format!("check_synthesize_{}", s), format!("error: {}", err));
                }
            }
        });
    }
    Ok(())
}
