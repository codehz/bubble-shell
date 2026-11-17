fn main() {
    println!("cargo:rerun-if-changed=../schema/src");
    println!("cargo:rerun-if-changed=../schema/src/codegen");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let schema = bubble_schema::bubble_shell_schema();
    let prim_spec = bubble_schema::codegen::prim_spec::generate(&schema);
    std::fs::write(format!("{out_dir}/prim_spec.rs"), prim_spec).expect("write prim_spec.rs");
}
