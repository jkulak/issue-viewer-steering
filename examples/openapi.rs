//! Print the OpenAPI description of the routes; `make openapi` writes it to openapi.json.

fn main() {
    let schema = issue_viewer::web::openapi::schema();
    println!(
        "{}",
        serde_json::to_string_pretty(&schema).expect("a schema is plain JSON")
    );
}
