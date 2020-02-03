fn main() {
    prost_build::compile_protos(&["src/worker.proto"], &["src/"]).unwrap();
}
