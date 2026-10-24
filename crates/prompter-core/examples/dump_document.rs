//! Dev helper: parse a Markdown script and print the `Document` as JSON.
//!
//! Usage: cargo run -p prompter-core --example dump_document -- examples/demo-nl.md
//! Used to refresh `ui/src/ipc/mock-document.json` for the browser-only dev mode.

use prompter_core::{Document, Lang};

fn main() {
    let path = std::env::args().nth(1).expect("usage: dump_document <script.md>");
    let text = std::fs::read_to_string(&path).expect("cannot read script");
    let doc = Document::parse(&path, &text, Lang::Nl);
    println!("{}", serde_json::to_string_pretty(&doc).expect("serialize"));
}
