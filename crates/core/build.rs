fn main() {
    println!("cargo:rerun-if-changed=../schema/src");
    println!("cargo:rerun-if-changed=../schema/src/codegen");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let schema = bubble_schema::bubble_shell_schema();
    let prop_enum = bubble_schema::codegen::prop_enum::generate(&schema);
    std::fs::write(format!("{out_dir}/prop_enum.rs"), prop_enum).expect("write prop_enum.rs");
    let dts = bubble_schema::codegen::dts::generate(&schema);
    std::fs::write(format!("{out_dir}/bubble-shell-generated.d.ts"), dts)
        .expect("write bubble-shell-generated.d.ts");
}
