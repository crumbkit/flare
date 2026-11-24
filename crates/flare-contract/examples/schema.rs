//! Prints the JSON Schema that `schema/flare-api.schema.json` commits.

#[expect(
    clippy::print_stdout,
    reason = "the example writes the schema to standard output"
)]
fn main() {
    print!("{}", flare_contract::schema::generate());
}
