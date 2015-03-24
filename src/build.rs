extern crate capnpc;

use std::path::Path;

fn main() {
    capnpc::compile(&Path::new("."), &[Path::new("api.capnp")]).unwrap();
}
