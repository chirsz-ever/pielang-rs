//! Procedural macros for pielang.
//!
//! Currently provides `#[tc_log]` — an attribute macro that injects
//! indented `log::trace!` calls at function entry/exit, and manages the
//! `INDENT` thread-local via an `IndentGuard`.
//!
//! Usage:
//!
//! ```ignore
//! // Entry log only:
//! #[tc_log("synthesize {}", dpp(e, env))]
//! fn synthesize(e: &Expr<M>, env: &Env) -> (Type<Never>, Expr<Never>) { ... }
//!
//! // Entry + exit log; the exit format can reference the return value
//! // through the identifier `ret`:
//! #[tc_log(
//!     "synthesize {}", dpp(e, env);
//!     "=> ty={}, expr={}", dpp(&ret.0, env), dpp(&ret.1, env)
//! )]
//! #[throws]
//! fn synthesize(e: &Expr<M>, env: &Env) -> (Type<Never>, Expr<Never>) { ... }
//! ```
//!
//! Notes:
//! * The generated code expects `INDENT` and `IndentGuard` to be
//!   accessible via `crate::type_check`.
//! * The exit log is only emitted when the function body evaluates
//!   normally to a value. Early exits caused by `throw!` / `?` bypass
//!   the exit log (the indent is still restored correctly through
//!   `Drop`).

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{parse_macro_input, Expr, ItemFn, LitStr, Token};

/// Parsed form of `#[tc_log(...)]` attribute.
///
/// Grammar:
///   Args := EntryPart (`;` ExitPart)?
///   EntryPart := LitStr (`,` Expr)*
///   ExitPart  := LitStr (`,` Expr)*
struct TcLogArgs {
    entry_fmt: LitStr,
    entry_args: Vec<Expr>,
    exit: Option<(LitStr, Vec<Expr>)>,
}

impl Parse for TcLogArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let entry_fmt: LitStr = input.parse()?;
        let mut entry_args = Vec::new();
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() || input.peek(Token![;]) {
                break;
            }
            entry_args.push(input.parse::<Expr>()?);
        }
        let exit = if input.peek(Token![;]) {
            input.parse::<Token![;]>()?;
            let exit_fmt: LitStr = input.parse()?;
            let mut exit_args = Vec::new();
            while input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
                if input.is_empty() {
                    break;
                }
                exit_args.push(input.parse::<Expr>()?);
            }
            Some((exit_fmt, exit_args))
        } else {
            None
        };
        Ok(TcLogArgs {
            entry_fmt,
            entry_args,
            exit,
        })
    }
}

#[proc_macro_attribute]
pub fn tc_log(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as TcLogArgs);
    let mut func = parse_macro_input!(item as ItemFn);

    let entry_fmt_lit = &args.entry_fmt;
    // let entry_fmt_str = format!("{{:indent$}}{}", entry_fmt_lit.value());
    let entry_fmt_str = format!("{{}}{}", entry_fmt_lit.value());
    let entry_fmt = LitStr::new(&entry_fmt_str, entry_fmt_lit.span());
    let entry_args = &args.entry_args;

    let block = &func.block;
    let block_span = block.span();

    let new_body: TokenStream2 = if let Some((exit_fmt_lit, exit_args)) = &args.exit {
        quote_spanned! { block_span =>
            {
                log::trace!(
                    #entry_fmt,
                    "│".repeat(crate::type_check::INDENT.get()) + "┌",
                    #(#entry_args,)*
                    // indent = crate::type_check::INDENT.get(),
                );
                let __tc_log_guard = crate::type_check::IndentGuard::new();
                #[allow(unreachable_code, unused_variables)]
                let ret = #block;
                #[allow(unreachable_code)]
                {
                    drop(__tc_log_guard);
                    log::trace!(
                        concat!(#entry_fmt, " ", #exit_fmt_lit),
                        "│".repeat(crate::type_check::INDENT.get()) + "└",
                        #(#entry_args,)*
                        #(#exit_args,)*
                        // indent = crate::type_check::INDENT.get(),
                    );
                    ret
                }
            }
        }
    } else {
        quote_spanned! { block_span =>
            {
                log::trace!(
                    #entry_fmt,
                    "│".repeat(crate::type_check::INDENT.get()),
                    #(#entry_args,)*
                    // indent = crate::type_check::INDENT.get(),
                );
                let __tc_log_guard = crate::type_check::IndentGuard::new();
                #[allow(unreachable_code, unused_variables)]
                let ret = #block;
                #[allow(unreachable_code)]
                {
                    drop(__tc_log_guard);
                    ret
                }
            }
        }
    };

    func.block = syn::parse2(new_body).expect("tc_log: failed to build new function body");
    TokenStream::from(quote!(#func))
}
