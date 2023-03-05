use libbpf_cargo::SkeletonBuilder;

const SRC: &str = "./src/bpf/bpftune.bpf.c";

fn main() {
    SkeletonBuilder::new()
        .source(SRC)
        .build_and_generate("./src/bpf/bpftune.skel.rs")
        .expect("bpf compilation failed");
    println!("cargo:rerun-if-changed={}", SRC);
    println!("cargo:rerun-if-changed=./src/bpf/bpftune.h");
}
