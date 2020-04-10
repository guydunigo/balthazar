fn main() {
    prost_build::compile_protos(
        &[
            "src/worker.proto",
            "src/manager.proto",
            "src/smartcontracts.proto",
        ],
        &["src/"],
    )
    .unwrap();
}
