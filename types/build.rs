use std::{io::Error, result::Result};

use glob::glob;

fn main() -> Result<(), Error> {
    let proto_paths: Vec<_> = glob("protobuf/**/*.proto")
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let proto_directories: Vec<_> = std::iter::once(std::path::PathBuf::from("protobuf"))
        .chain(glob("protobuf/**").unwrap().map(Result::unwrap))
        .collect();
    // dbg!(proto_paths.clone());
    // dbg!(proto_directories.clone());
    prost_build::compile_protos(&proto_paths, &proto_directories)
}
