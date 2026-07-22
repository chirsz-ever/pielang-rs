# pielang-rs

RIIR [Pie: A Little Language with Dependent Types](https://github.com/the-little-typer/pie).

[The Pie Reference](https://docs.racket-lang.org/pie)

## Running

```txt
cargo run -- [FLAGS] [OPTIONS] [--] [FILE]

FLAGS:
    -c, --check      Only run check type
    -h, --help       Prints help information
    -i, --repl       Open REPL
    -V, --version    Prints version information

OPTIONS:
    -e, --eval <exprs>...    Read and eval a pie expression from command line arguments

ARGS:
    <FILE>    Input file, use `-` to read from stdin
```

## Passes

- source code into `pielang::ast::Expr`
  - addtional checks for global statements
- checking `pielang::ast::Expr` syntax
  - checking the λ-expressions do not use built-in names as variable names
  - checking built-in names have correct number of arguments
  - checking no unbound variables
- Type checking `pielang::ast::Expr` and elaboration into `pielang::core::Expr`
  - `core::Expr` uses de Bruijn indices for variables

## TODO

- [x] `the` expression
  - [x] `(the T e)`
  - [x] `(the U T)`
- [x] Absurd
  - [x] `Absurd`
  - [x] `ind-Absurd`
- [x] Trivial
  - [x] `Trivial`
  - [x] `sole`
- [x] Atoms
  - [x] `Atom`
  - [x] `quote`, atom literals
- [ ] Natural Numbers
  - [x] `Nat`
  - [x] `zero`, `(add1 n)`, natural literals
  - [ ] `which-Nat`
  - [ ] `iter-Nat`
  - [ ] `rec-Nat`
  - [ ] `ind-Nat`
- [ ] Pairs
- [ ] Functions
- [ ] Lists
- [ ] Vectors
- [ ] Either
- [ ] Equality
- [ ] Universe
- [ ] `claim` and `define`
- [ ] `check-same`
- [ ] Eval
- [ ] `TODO`
- [ ] Extension: Type in Type
- [ ] Extension: Universe Hierarchy
- [ ] Extension: User Defined Inductive Datatypes
