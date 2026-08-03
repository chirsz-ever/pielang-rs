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
- [x] Natural Numbers
  - [x] `Nat`
  - [x] `zero`, `(add1 n)`, natural literals
  - [x] `which-Nat`
  - [x] `iter-Nat`
  - [x] `rec-Nat`
  - [x] `ind-Nat`
- [x] Pairs
  - [x] `Pair`
  - [x] `cons`
  - [x] `car`, `cdr`
  - [ ] `Σ`
- [x] Functions
  - [x] `->`
  - [x] `λ`
  - [x] application
  - [x] `Π`
- [ ] Lists
  - [x] `List`
  - [x] `nil`, `::`
  - [x] `rec-List`
  - [ ] `ind-List`
- [ ] Vectors
  - [x] `Vec`
  - [x] `vecnil`, `vec::`
  - [ ] `ind-Vec`
- [ ] Either
  - [ ] `Either`
  - [ ] `left`, `right`
  - [ ] `ind-Either`
- [ ] Equality
  - [x] `=`
  - [x] `same`
  - [x] `replace`
  - [x] `cong`
  - [x] `symm`
  - [ ] `trans`
  - [ ] `ind-=`
- [ ] Universe
- [x] `claim` and `define`
- [x] `check-same`
- [x] Eval
- [ ] `TODO`
- [ ] Extension: Type in Type
- [ ] Extension: Universe Hierarchy
- [ ] Extension: User Defined Inductive Datatypes
