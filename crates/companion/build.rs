fn main() {
    println!("cargo:rustc-env=KEYSIDE_TARGET={}", std::env::var("TARGET").unwrap_or_default());
}
