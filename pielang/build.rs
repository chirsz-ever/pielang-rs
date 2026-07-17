extern crate lalrpop;

fn main() {
    lalrpop::Configuration::new()
        .emit_rerun_directives(true)
        .process()
        .unwrap();
}
